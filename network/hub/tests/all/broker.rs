//! The Discord broker on a running hub: the bot token read from a file and never logged, the broker
//! off without it, the application read from Discord at the start, Discord's `429` waited out on
//! its route, and the client's calls as Discord's API takes them; rooms mirrored into Discord
//! servers' channels (the `bridge` module below), and tells into a person's Discord direct messages
//! and back (`bridge::tells`).
//! Fixture: a stand-in for Discord's API on loopback (never Discord itself, `bridge::stand_in`),
//! which keeps its servers, channels, webhooks and direct messages in memory, records every call,
//! and answers `429` where a test asks it to; a hub on loopback over an in-memory registry, or over
//! a registry file a test restarts it on; or the operator tests' hub (a stand-in for Discord's
//! sign-in, a clock the test turns) with the broker on.

use std::io::Read as _;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use dereth_network_hub::broker::{Broker, BrokerError, Rest};
use dereth_network_hub::registry::{Registration, Registry};
use dereth_network_hub::server::{start_with, HubConfig, Running};
use serde_json::{json, Value};

use super::operators::{call, hub_with, op, sign_in};

/// The bot token the stand-in takes.
const BOT_TOKEN: &str = "stand-in-bot-token.Zx9q";

/// The broker's settings for the stand-in at `stub`, trying again after 200 ms and reading every
/// 50 ms.
fn broker_for(stub: &super::operators::Stub) -> Broker {
    Broker {
        api: format!("http://{}/api", stub.addr),
        retry: Duration::from_millis(200),
        poll: Duration::from_millis(50),
        ..Broker::new(BOT_TOKEN.into())
    }
}

fn registration(slug: &str) -> Registration {
    Registration::from_json(&json!({
        "slug": slug, "name": format!("World {slug}"), "ruleset": "PvE",
        "endpoint": { "mode": "manual", "address": "192.0.2.1", "port": 9000 },
        "emulator": { "family": "ACE" },
    }))
    .expect("a registration")
}

/// A folder of this test's own, empty.
fn folder(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("dereth-hub-broker-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a folder");
    dir
}

/// A hub over the registry file `db`, with `broker`, and no sign-in, its registry first as
/// `prepare` leaves it.
fn hub_on(db: &str, broker: Broker, prepare: impl FnOnce(&Registry)) -> Running {
    let registry = Registry::open(db, "dereth.network", Box::new(std::time::SystemTime::now))
        .expect("a registry");
    prepare(&registry);
    start_with(
        HubConfig {
            listen: "127.0.0.1:0".parse().expect("an address"),
            db: db.to_owned(),
            broker: Some(broker),
            ..HubConfig::default()
        },
        registry,
    )
    .expect("the hub starts")
}

/// Waits, up to 30 seconds, for `ready` to give something.
fn within<T>(what: &str, mut ready: impl FnMut() -> Option<T>) -> T {
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(t) = ready() {
            return t;
        }
        assert!(Instant::now() < until, "no {what} within 30 seconds");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The hub binary's output, gathered while it runs.
struct Output {
    log: Arc<Mutex<String>>,
    readers: Vec<std::thread::JoinHandle<()>>,
}

impl Output {
    /// Gathers `child`'s standard error and standard output.
    fn of(child: &mut std::process::Child) -> Self {
        let log = Arc::new(Mutex::new(String::new()));
        let mut readers = Vec::new();
        for mut pipe in [
            Box::new(child.stderr.take().expect("stderr")) as Box<dyn std::io::Read + Send>,
            Box::new(child.stdout.take().expect("stdout")),
        ] {
            let log = Arc::clone(&log);
            readers.push(std::thread::spawn(move || {
                let mut buf = [0u8; 4096];
                while let Ok(n) = pipe.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    log.lock()
                        .expect("the log")
                        .push_str(&String::from_utf8_lossy(&buf[..n]));
                }
            }));
        }
        Self { log, readers }
    }

    /// Waits until the log holds `what`.
    fn says(&self, what: &str) {
        within(what, || {
            self.log
                .lock()
                .expect("the log")
                .contains(what)
                .then_some(())
        });
    }

    /// Everything `child` wrote, once it is stopped.
    fn all(self, child: &mut std::process::Child) -> String {
        let _ = child.kill();
        let _ = child.wait();
        for r in self.readers {
            let _ = r.join();
        }
        let log = self.log.lock().expect("the log").clone();
        log
    }
}

/// Spec: hub: without the bot token the broker is off: the configuration says every feature is
/// off, the mirrors' routes answer `503`, nothing calls Discord's bot API, and sign-in works as it
/// did.
#[test]
fn without_the_bot_token_the_broker_is_off_and_sign_in_works_as_before() {
    let (stub, d) = bridge::stand_in();
    let f = hub_with(|_| {});
    let config = call(f.addr(), "GET", "/v1/config", &[], "").json();
    assert_eq!(config["authEnabled"], true);
    assert_eq!(
        config["broker"],
        json!({ "discordTells": false, "mirrors": false, "speak": false })
    );
    let admin = sign_in(&f, super::operators::ADMIN);
    assert_eq!(
        op(&f, &admin, "GET", "/v1/admin/discord", &Value::Null).status,
        503
    );
    assert_eq!(
        call(
            f.addr(),
            "POST",
            "/v1/discord/interactions",
            &[],
            r#"{"type":1}"#
        )
        .status,
        503,
        "no interactions without the bot token"
    );
    assert!(d.lock().expect("the stand-in").calls.is_empty());
    drop(stub);
}

/// Spec: hub: the bot token and the client secret are read from the files the hub's operator names
/// (white space at their ends trimmed), never from the command line, and neither is ever written
/// to the log; the bot token goes to Discord as the bot's credential; the reading interval needs
/// the bot token; an empty token file is refused, and so are the settings the admin routes hold.
#[test]
fn the_bot_token_is_read_from_its_file_and_never_logged() {
    let dir = folder("secrets");
    let (stub, d) = bridge::stand_in();
    let token_file = dir.join("discord-bot-token");
    std::fs::write(&token_file, format!("{BOT_TOKEN}\n")).expect("written");
    let secret_file = dir.join("discord-client-secret");
    let secret = "stand-in-client-secret-7c1";
    std::fs::write(&secret_file, secret).expect("written");
    let db = dir.join("hub.sqlite");
    bridge::server_with_channel(&d, &[]);
    {
        let r = Registry::open(
            db.to_str().expect("a path"),
            "dereth.network",
            Box::new(std::time::SystemTime::now),
        )
        .expect("a registry");
        r.set_mirror(
            None,
            "global",
            &dereth_network_hub::bridge::Placement::bot(bridge::CHANNEL, bridge::GUILD),
            None,
        )
        .expect("mirrored");
    }
    let hub = env!("CARGO_BIN_EXE_dereth-network-hub");
    let path = |p: &std::path::Path| p.to_str().expect("a path").to_owned();
    let mut child = Command::new(hub)
        .args([
            "serve",
            "--listen",
            "127.0.0.1:0",
            "--run-for",
            "120",
            "--db",
            &path(&db),
            "--discord-bot-token-file",
            &path(&token_file),
            "--discord-api",
            &format!("http://{}/api", stub.addr),
            "--discord-client-id",
            "app",
            "--discord-client-secret-file",
            &path(&secret_file),
            "--public-url",
            "https://hub.test",
            "--site-origin",
            "https://site.test",
        ])
        .env_remove("DERETH_HUB_DISCORD_BOT_TOKEN")
        .env_remove("DERETH_HUB_DISCORD_CLIENT_SECRET")
        .env_remove("DERETH_HUB_DISCORD_CLIENT_ID")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the hub runs");
    let out = Output::of(&mut child);
    out.says("made the Discord webhook");
    let log = out.all(&mut child);
    assert!(log.contains("Discord broker on"), "{log}");
    assert!(log.contains("tells into Discord on"), "{log}");
    assert!(
        !log.contains(BOT_TOKEN),
        "the bot token is never logged:\n{log}"
    );
    assert!(
        !log.contains(secret),
        "the client secret is never logged:\n{log}"
    );
    let calls = d.lock().expect("the stand-in").calls.clone();
    let read = calls
        .iter()
        .find(|c| c.method == "GET" && c.path == "/api/applications/@me")
        .expect("the application was read");
    assert_eq!(read.auth, "Bot", "the token from the file, trimmed");
    assert!(read.agent.starts_with("DiscordBot (https://site.test, "));

    let refused = |args: &[&str]| {
        let out = Command::new(hub)
            .args(args)
            .env_remove("DERETH_HUB_DISCORD_BOT_TOKEN")
            .output()
            .expect("the hub runs");
        assert!(!out.status.success(), "{args:?}");
        String::from_utf8_lossy(&out.stderr).into_owned()
    };
    let db = path(&db);
    let serve = |more: &[&str]| {
        let mut args = vec!["serve", "--listen", "127.0.0.1:0", "--run-for", "0", "--db"];
        args.push(&db);
        args.extend_from_slice(more);
        refused(&args)
    };
    let e = serve(&["--discord-poll-secs", "1"]);
    assert!(e.contains("bot token"), "{e}");
    let empty = dir.join("empty");
    std::fs::write(&empty, "\n").expect("written");
    let e = serve(&["--discord-bot-token-file", &path(&empty)]);
    assert!(e.contains("holds no token"), "{e}");
    let e = serve(&["--discord-bot-token", BOT_TOKEN]);
    assert!(!e.contains(BOT_TOKEN), "{e}");
    for gone in [
        "--discord-room",
        "--discord-channel",
        "--discord-avatar-url",
        "--discord-quota",
        "--discord-world-rooms",
    ] {
        let e = serve(&[gone, "global"]);
        assert!(e.contains("unexpected"), "{gone}: {e}");
    }
    drop(stub);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Spec: hub: after a `429` the broker asks Discord nothing on that route until the wait it names
/// is over, and on any route after a global one; other routes go on.
#[test]
fn a_429_is_waited_out_on_its_route_without_asking_discord() {
    let (stub, d) = bridge::stand_in();
    bridge::server_with_channel(&d, &[]);
    let rest = Rest::new(broker_for(&stub));
    let calls = || d.lock().expect("the stand-in").calls.len();
    let channel = format!("/channels/{}", bridge::CHANNEL);
    d.lock()
        .expect("the stand-in")
        .busy
        .insert(channel.clone(), (30, 1));
    assert_eq!(rest.channel(bridge::CHANNEL), Err(BrokerError::Wait(30)));
    let before = calls();
    assert!(matches!(
        rest.channel(bridge::CHANNEL),
        Err(BrokerError::Wait(s)) if (1..=30).contains(&s)
    ));
    assert_eq!(calls(), before, "Discord was not asked");
    rest.application().expect("another route goes on");
    rest.guild(bridge::GUILD).expect("and another");
    assert_eq!(calls(), before + 2);

    // A global wait holds every route.
    let guild = format!("/guilds/{}", bridge::GUILD);
    {
        let mut d = d.lock().expect("the stand-in");
        d.busy.insert(guild.clone(), (30, 1));
        d.global.insert(guild);
    }
    assert_eq!(rest.guild(bridge::GUILD), Err(BrokerError::Wait(30)));
    let before = calls();
    assert!(matches!(rest.application(), Err(BrokerError::Wait(_))));
    assert!(matches!(
        rest.webhooks(bridge::CHANNEL),
        Err(BrokerError::Wait(_))
    ));
    assert_eq!(calls(), before, "Discord was not asked");

    // A wait of a second is over after a second.
    let rest = Rest::new(broker_for(&stub));
    d.lock().expect("the stand-in").busy.insert(channel, (1, 1));
    let asked = Instant::now();
    assert_eq!(rest.channel(bridge::CHANNEL), Err(BrokerError::Wait(1)));
    let seen = within("the wait's end", || rest.channel(bridge::CHANNEL).ok());
    assert!(asked.elapsed() >= Duration::from_millis(900));
    assert_eq!(seen["id"], bridge::CHANNEL);
    drop(stub);
}

/// Spec: hub: the broker's client speaks Discord's routes as the bot: the application, a channel,
/// a server, a member, a channel's webhooks listed and made, and the bot's own direct messages and
/// their lines; a webhook's line and a webhook read by its token carry no credential but the
/// webhook's own; a webhook Discord does not have under its token is none, and no path takes
/// anything but a Discord id.
#[test]
fn the_brokers_client_speaks_discords_routes() {
    let (stub, d) = bridge::stand_in();
    bridge::server_with_channel(&d, &[]);
    let rest = Rest::new(broker_for(&stub));
    let app = rest.application().expect("the application");
    assert_eq!(
        (app.id.as_str(), app.bot_id.as_str()),
        (bridge::APP, bridge::BOT)
    );
    assert_eq!(
        rest.channel(bridge::CHANNEL).expect("seen")["guild_id"],
        bridge::GUILD
    );
    assert_eq!(
        rest.guild(bridge::GUILD).expect("seen")["owner_id"],
        bridge::GUILD_OWNER
    );
    assert!(rest
        .guild_member(bridge::GUILD, bridge::BOT)
        .expect("asked")
        .is_some());
    assert_eq!(
        rest.guild_member(bridge::GUILD, "80351110224678999"),
        Ok(None)
    );
    assert!(rest.webhooks(bridge::CHANNEL).expect("listed").is_empty());
    let hook = rest
        .create_webhook(bridge::CHANNEL, "Dereth Network")
        .expect("made");
    assert_eq!(rest.webhooks(bridge::CHANNEL).expect("listed").len(), 1);
    rest.execute_webhook(
        &hook,
        "hello",
        Some("Ann@frost"),
        Some("https://hub.test/a.png"),
    )
    .expect("said");
    let (read, channel) = rest.webhook(&hook.id, hook.token()).expect("read");
    assert_eq!(
        (read.guild.as_str(), channel.as_str()),
        (bridge::GUILD, bridge::CHANNEL)
    );
    assert_eq!(
        rest.webhook(&hook.id, "not-its-token"),
        Err(BrokerError::NotFound)
    );
    let user = "80351110224678912";
    d.lock()
        .expect("the stand-in")
        .reachable
        .insert(user.into());
    let dm = rest.open_dm(user).expect("opened");
    rest.send_dm(&dm, "a tell").expect("said");
    assert_eq!(rest.channel_lines(&dm, None, 10).expect("read").len(), 1);
    assert!(matches!(
        rest.channel("../users/@me"),
        Err(BrokerError::Failed(_))
    ));
    let d = d.lock().expect("the stand-in");
    let said = d.webhook_calls();
    assert_eq!(said[0].auth, "", "no credential but the webhook's own");
    assert_eq!(said[0].body["username"], "Ann@frost");
    assert_eq!(said[0].body["avatar_url"], "https://hub.test/a.png");
    assert_eq!(said[0].body["allowed_mentions"], json!({ "parse": [] }));
    let by_token: Vec<_> = d
        .calls
        .iter()
        .filter(|c| c.method == "GET" && c.path.starts_with("/api/webhooks/"))
        .collect();
    assert_eq!(by_token.len(), 2);
    assert!(
        by_token.iter().all(|c| c.auth.is_empty()),
        "read by its token alone"
    );
    assert!(d
        .calls
        .iter()
        .filter(|c| !c.path.starts_with("/api/webhooks/"))
        .all(|c| c.auth == "Bot"));
    assert!(d.calls.iter().all(|c| c.agent.starts_with("DiscordBot (")));
    drop(stub);
}

// ---- the broker on ------------------------------------------------------------------------------

/// Spec: hub: with sign-in and the bot token, the configuration says each of the broker's features
/// is on.
#[test]
fn with_sign_in_and_the_bot_token_the_broker_is_on() {
    let (stub, _) = bridge::stand_in();
    let broker = broker_for(&stub);
    let f = hub_with(move |c| c.broker = Some(broker));
    let config = call(f.addr(), "GET", "/v1/config", &[], "").json();
    assert_eq!(
        config["broker"],
        json!({ "discordTells": true, "mirrors": true, "speak": true })
    );
    drop(stub);
}

// ---- rooms mirrored into Discord ------------------------------------------------------------------

mod bridge {
    //! Rooms mirrored into Discord servers' channels on a running hub, from the game into Discord
    //! alone: a line said through the bot's own webhook under each speaker's address, calling
    //! nobody; Discord's waits on the webhook kept; a line said in the game going to every mirror
    //! of its room, and no mirror's channel ever read; a world's mirrors set only by its operator,
    //! where both the operator and the bot may manage the channel's webhooks, or through a
    //! webhook the operator made, kept secret; the admins' own mirrors and the picture set live,
    //! and any world operator's mirror taken away by them; and `discord` reserved.
    //! Fixture: a stand-in for Discord's API on loopback (never Discord itself) that keeps servers,
    //! with their roles, members, channels, webhooks and lines, answering the bot as its
    //! permissions there allow, worked out as the hub works them out, and the bot's direct
    //! messages with users, refusing a user who lets the bot message them not, and answers `429`
    //! where a test asks it to; the operator tests' hub with the broker on and a short read
    //! interval for direct messages; worlds linked through the conformance suite's links.

    use std::collections::{BTreeMap, HashMap, HashSet};
    use std::net::TcpStream;
    use std::process::{Command, Stdio};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use dereth_network_conformance::hub::Link;
    use dereth_network_hub::bridge::{permissions, Placement, MANAGE_WEBHOOKS, VIEW_CHANNEL};
    use dereth_network_hub::registry::{Registration, Registry};
    use dereth_network_hub::server::Running;
    use dereth_network_protocol::{
        Address, ChannelOutcome, Federation, Message, Outcome, RoomsMessage,
    };
    use serde_json::{json, Value};

    use super::super::operators::{answer, call, hub_with, op, sign_in, Fixture, Got, Stub, ADMIN};
    use super::{broker_for, folder, hub_on, within, Output, BOT_TOKEN};

    /// How long a test waits for something the hub does on its own threads.
    const WAIT: Duration = Duration::from_secs(30);

    /// The stand-in's application, and its bot user.
    pub(super) const APP: &str = "1558376950000000000";
    /// See [`APP`].
    pub(super) const BOT: &str = "1558376950000000001";
    /// The roles of the stand-in's servers: one that manages webhooks, and one that does nothing.
    const HOOKS_ROLE: &str = "1558376952000000001";
    /// See [`HOOKS_ROLE`].
    const PLAIN_ROLE: &str = "1558376952000000002";

    /// A call the stand-in took.
    #[derive(Debug, Clone)]
    pub(super) struct Call {
        pub(super) method: String,
        pub(super) path: String,
        /// `Bot`, the bearer token, or nothing.
        pub(super) auth: String,
        pub(super) agent: String,
        pub(super) body: Value,
        at: Instant,
    }

    /// A Discord server: its owner, its roles (its everyone role's id is the server's) and its
    /// members by user id.
    #[derive(Debug, Default)]
    struct Server {
        owner: String,
        roles: Vec<Value>,
        members: HashMap<String, Value>,
    }

    /// A server's channel: its server, its kind, its permission overwrites and its lines.
    #[derive(Debug, Default)]
    struct Chan {
        guild: String,
        kind: u64,
        overwrites: Value,
        lines: Vec<Value>,
    }

    /// The bot's direct messages with one user: the user, and the lines, oldest first.
    #[derive(Debug, Default)]
    pub(super) struct Dm {
        pub(super) recipient: String,
        pub(super) lines: Vec<Value>,
    }

    /// A channel's webhook, and who made it.
    #[derive(Debug, Clone)]
    pub(super) struct Hook {
        id: String,
        token: String,
        channel: String,
        guild: String,
        by: String,
        name: String,
    }

    /// What the stand-in keeps.
    #[derive(Debug, Default)]
    pub(super) struct Discord {
        pub(super) calls: Vec<Call>,
        servers: BTreeMap<String, Server>,
        chans: BTreeMap<String, Chan>,
        hooks: Vec<Hook>,
        /// The bot's direct messages, by their channel's id.
        pub(super) dms: BTreeMap<String, Dm>,
        /// The users who let the bot message them (they share a server with it and allow it).
        pub(super) reachable: HashSet<String>,
        /// Paths answered `429`: with how many seconds, and how many more times.
        pub(super) busy: HashMap<String, (u64, usize)>,
        /// The paths whose `429` is for every route.
        pub(super) global: HashSet<String>,
        /// The paths answered `429`, and when.
        busied: Vec<(String, Instant)>,
        /// Paths whose next answer says they have no call left for this many seconds.
        spent: HashMap<String, f64>,
        /// The commands the application's were last replaced with.
        pub(super) commands: Option<Value>,
        /// The answers to interactions edited, each its token and what it was edited to say.
        pub(super) edits: Vec<(String, String)>,
        next: u64,
    }

    impl Discord {
        pub(super) fn id(&mut self) -> String {
            self.next += 1;
            (1_558_376_954_619_629_600 + self.next).to_string()
        }

        /// The server `guild` as Discord describes it to the bot, and the channel `channel` of it.
        fn described(&self, channel: &str) -> Option<(Value, Value)> {
            let ch = self.chans.get(channel)?;
            let g = self.servers.get(&ch.guild)?;
            Some((
                json!({ "id": ch.guild, "owner_id": g.owner, "roles": g.roles }),
                json!({ "id": channel, "guild_id": ch.guild, "type": ch.kind,
                        "permission_overwrites": ch.overwrites }),
            ))
        }

        /// The bot's permissions in `channel`.
        fn bot_may(&self, channel: &str) -> u64 {
            let Some((guild, ch)) = self.described(channel) else {
                return 0;
            };
            let Some(member) = self
                .chans
                .get(channel)
                .and_then(|c| self.servers.get(&c.guild))
                .and_then(|g| g.members.get(BOT))
            else {
                return 0;
            };
            permissions(&guild, BOT, member, &ch)
        }

        /// The lines of the server channel `channel`, oldest first.
        fn channel_lines(&self, channel: &str) -> Vec<Value> {
            self.chans
                .get(channel)
                .map(|c| c.lines.clone())
                .unwrap_or_default()
        }

        /// How many times the server channel `channel`'s lines were read.
        fn channel_reads(&self, channel: &str) -> usize {
            let path = format!("/api/channels/{channel}/messages?");
            self.calls
                .iter()
                .filter(|c| c.method == "GET" && c.path.starts_with(&path))
                .count()
        }

        /// The lines said through webhooks, as the hub sent them.
        pub(super) fn webhook_calls(&self) -> Vec<Call> {
            self.calls
                .iter()
                .filter(|c| c.method == "POST" && c.path.starts_with("/api/webhooks/"))
                .cloned()
                .collect()
        }

        /// How many calls of `method` on a path starting `path` it took.
        pub(super) fn count(&self, method: &str, path: &str) -> usize {
            self.calls
                .iter()
                .filter(|c| c.method == method && c.path.starts_with(path))
                .count()
        }

        /// The path of `channel`'s webhook, as the stand-in answers `429`s on it.
        fn hook_path(&self, channel: &str) -> String {
            let hook = self
                .hooks
                .iter()
                .find(|h| h.channel == channel)
                .expect("the webhook");
            format!("/webhooks/{}/{}", hook.id, hook.token)
        }
    }

    pub(super) type Shared = Arc<Mutex<Discord>>;

    /// A Discord user who is not a game's: shown as `shown`.
    fn discord_user(id: &str, shown: &str, username: &str) -> Value {
        json!({ "id": id, "username": username, "global_name": shown, "bot": false })
    }

    /// The stand-in for Discord's API, at `http://<addr>/api`: the application, a channel, a
    /// server, a member, a channel's webhooks made and listed, a webhook read with its token, a
    /// line said through a webhook (refusing a name holding `discord` or `clyde`, as Discord does),
    /// and the bot's direct messages and their lines, newest first, after a line or the newest.
    pub(super) fn stand_in() -> (Stub, Shared) {
        let discord: Shared = Arc::default();
        let d = Arc::clone(&discord);
        let stub = Stub::start(Arc::new(move |got: &Got, s: &mut TcpStream| {
            let mut d = d.lock().expect("the stand-in");
            let auth = got.header("authorization").unwrap_or("").to_owned();
            let body: Value = serde_json::from_str(&got.body).unwrap_or(Value::Null);
            let is_bot = auth == format!("Bot {BOT_TOKEN}");
            d.calls.push(Call {
                method: got.method.clone(),
                path: got.path.clone(),
                auth: if is_bot { "Bot".into() } else { auth.clone() },
                agent: got.header("user-agent").unwrap_or("").to_owned(),
                body: body.clone(),
                at: Instant::now(),
            });
            let path = got.path.strip_prefix("/api").unwrap_or("");
            let (route, query) = path.split_once('?').unwrap_or((path, ""));
            let segs: Vec<&str> = route.trim_matches('/').split('/').collect();
            if let Some((secs, left)) = d.busy.get_mut(route).filter(|(_, left)| *left > 0) {
                *left -= 1;
                let secs = *secs;
                let global = d.global.contains(route);
                d.busied.push((route.to_owned(), Instant::now()));
                return answer(
                    s,
                    429,
                    &format!(
                        "Retry-After: {secs}\r\nX-RateLimit-Global: {global}\r\nContent-Type: application/json\r\n"
                    ),
                    &json!({ "message": "You are being rate limited.", "retry_after": secs, "global": global })
                        .to_string(),
                );
            }
            if !servers(&mut d, &got.method, &segs, query, &body, is_bot, s) {
                answer(
                    s,
                    404,
                    "Content-Type: application/json\r\n",
                    &json!({ "message": "404: Not Found", "code": 0 }).to_string(),
                );
            }
        }));
        (stub, discord)
    }

    /// The stand-in's routes (true when the call was one).
    #[allow(clippy::too_many_lines)]
    fn servers(
        d: &mut Discord,
        method: &str,
        segs: &[&str],
        query: &str,
        body: &Value,
        is_bot: bool,
        s: &mut TcpStream,
    ) -> bool {
        let json = |s: &mut TcpStream, status: u16, v: &Value| {
            answer(
                s,
                status,
                "Content-Type: application/json\r\n",
                &v.to_string(),
            );
        };
        let missing = json!({ "message": "Missing Access", "code": 50_001 });
        let lacking = json!({ "message": "Missing Permissions", "code": 50_013 });
        let unknown = |what: &str| json!({ "message": format!("Unknown {what}"), "code": 10_003 });
        let param = |name: &str| {
            query
                .split('&')
                .find_map(|kv| kv.strip_prefix(&format!("{name}=")))
                .and_then(|v| v.parse::<u64>().ok())
        };
        let picked = |lines: Vec<Value>| {
            let id_of = |l: &Value| l["id"].as_str().and_then(|i| i.parse::<u64>().ok());
            let limit = usize::try_from(param("limit").unwrap_or(50)).unwrap_or(50);
            let mut picked: Vec<Value> = match param("after") {
                Some(after) => lines
                    .into_iter()
                    .filter(|l| id_of(l).is_some_and(|i| i > after))
                    .take(limit)
                    .collect(),
                None => {
                    let skip = lines.len().saturating_sub(limit);
                    lines.into_iter().skip(skip).collect()
                }
            };
            picked.reverse();
            picked
        };
        let hook_json = |h: &Hook| {
            json!({ "id": h.id, "type": 1, "guild_id": h.guild, "channel_id": h.channel,
                    "name": h.name, "token": h.token, "user": { "id": h.by } })
        };
        match (method, segs) {
            ("POST", ["users", "@me", "channels"]) if is_bot => {
                let Some(user) = body["recipient_id"].as_str().map(str::to_owned) else {
                    json(
                        s,
                        400,
                        &json!({ "message": "Invalid Form Body", "code": 50_035 }),
                    );
                    return true;
                };
                let open = d
                    .dms
                    .iter()
                    .find(|(_, m)| m.recipient == user)
                    .map(|(id, _)| id.clone());
                let id = match open {
                    Some(id) => id,
                    None => {
                        let id = d.id();
                        d.dms.insert(
                            id.clone(),
                            Dm {
                                recipient: user.clone(),
                                lines: Vec::new(),
                            },
                        );
                        id
                    }
                };
                json(
                    s,
                    200,
                    &json!({ "id": id, "type": 1, "flags": 0, "recipients": [{ "id": user }] }),
                );
            }
            ("POST", ["channels", id, "messages"]) if is_bot && d.dms.contains_key(*id) => {
                let recipient = d.dms[*id].recipient.clone();
                if !d.reachable.contains(&recipient) {
                    json(
                        s,
                        403,
                        &json!({ "message": "Cannot send messages to this user", "code": 50_007 }),
                    );
                    return true;
                }
                let line_id = d.id();
                let line = json!({
                    "id": line_id, "type": 0, "channel_id": id, "content": body["content"],
                    "flags": body["flags"], "allowed_mentions": body["allowed_mentions"],
                    "author": { "id": BOT, "username": "Dereth Network", "bot": true },
                    "mentions": [], "attachments": [],
                });
                if let Some(dm) = d.dms.get_mut(*id) {
                    dm.lines.push(line.clone());
                }
                json(s, 200, &line);
            }
            ("GET", ["channels", id, "messages"]) if is_bot && d.dms.contains_key(*id) => {
                let lines = d.dms[*id].lines.clone();
                json(s, 200, &Value::Array(picked(lines)));
            }
            ("GET", ["applications", "@me"]) if is_bot => json(
                s,
                200,
                &json!({ "id": APP, "bot": { "id": BOT, "username": "Dereth Network" },
                         "verify_key": speak::public_key() }),
            ),
            ("PUT", ["applications", APP, "commands"]) if is_bot => {
                d.commands = Some(body.clone());
                json(s, 200, body);
            }
            ("PATCH", ["webhooks", APP, token, "messages", "@original"]) => {
                let content = body["content"].as_str().unwrap_or("").to_owned();
                d.edits.push(((*token).to_owned(), content.clone()));
                json(
                    s,
                    200,
                    &json!({ "id": d.id(), "content": content, "flags": 64 }),
                );
            }
            ("GET", ["channels", id]) if is_bot => match d.described(id) {
                Some(_) if d.bot_may(id) & VIEW_CHANNEL == 0 => json(s, 403, &missing),
                Some((_, ch)) => json(s, 200, &ch),
                None => json(s, 404, &unknown("Channel")),
            },
            ("GET", ["guilds", id]) if is_bot => match d.servers.get(*id) {
                Some(g) if g.members.contains_key(BOT) => json(
                    s,
                    200,
                    &json!({ "id": id, "owner_id": g.owner, "roles": g.roles }),
                ),
                Some(_) => json(s, 403, &missing),
                None => json(s, 404, &unknown("Guild")),
            },
            ("GET", ["guilds", id, "members", user]) if is_bot => {
                match d.servers.get(*id).and_then(|g| g.members.get(*user)) {
                    Some(m) => json(s, 200, m),
                    None => json(s, 404, &unknown("Member")),
                }
            }
            ("GET" | "POST", ["channels", id, "webhooks"]) if is_bot => {
                let may = d.bot_may(id);
                if may & VIEW_CHANNEL == 0 {
                    json(s, 403, &missing);
                    return true;
                }
                if may & MANAGE_WEBHOOKS == 0 {
                    json(s, 403, &lacking);
                    return true;
                }
                if method == "POST" {
                    let guild = d.chans[*id].guild.clone();
                    let hook = make_hook(d, id, &guild, BOT, body["name"].as_str().unwrap_or(""));
                    json(s, 200, &hook_json(&hook));
                } else {
                    let hooks: Vec<Value> = d
                        .hooks
                        .iter()
                        .filter(|h| h.channel == *id)
                        .map(hook_json)
                        .collect();
                    json(s, 200, &Value::Array(hooks));
                }
            }
            ("GET", ["webhooks", id, token]) => {
                match d.hooks.iter().find(|h| h.id == *id && h.token == *token) {
                    Some(h) => {
                        let mut v = hook_json(h);
                        v.as_object_mut().expect("an object").remove("user");
                        json(s, 200, &v);
                    }
                    None => json(
                        s,
                        404,
                        &json!({ "message": "Unknown Webhook", "code": 10_015 }),
                    ),
                }
            }
            ("POST", ["webhooks", id, token]) => {
                let Some(hook) = d
                    .hooks
                    .iter()
                    .find(|h| h.id == *id && h.token == *token)
                    .cloned()
                else {
                    json(
                        s,
                        404,
                        &json!({ "message": "Unknown Webhook", "code": 10_015 }),
                    );
                    return true;
                };
                let name = body["username"].as_str();
                if name.is_some_and(|n| {
                    let n = n.to_lowercase();
                    n.contains("discord") || n.contains("clyde")
                }) {
                    json(
                        s,
                        400,
                        &json!({ "code": 50_035, "message": "Invalid Form Body", "errors": {
                            "username": { "_errors": [{ "code": "USERNAME_INVALID_CONTAINS",
                                                        "message": "Username cannot contain \"discord\"" }] } } }),
                    );
                    return true;
                }
                let line_id = d.id();
                let line = json!({
                    "id": line_id, "type": 0, "channel_id": hook.channel, "content": body["content"],
                    "webhook_id": hook.id, "mentions": [], "attachments": [],
                    "author": { "id": hook.id, "username": name.unwrap_or(&hook.name) },
                });
                d.chans
                    .get_mut(&hook.channel)
                    .expect("its channel")
                    .lines
                    .push(line.clone());
                let spent = d
                    .spent
                    .remove(&format!("/webhooks/{id}/{token}"))
                    .map(|secs| {
                        format!("X-RateLimit-Remaining: 0\r\nX-RateLimit-Reset-After: {secs}\r\n")
                    })
                    .unwrap_or_default();
                answer(
                    s,
                    200,
                    &format!("{spent}Content-Type: application/json\r\n"),
                    &line.to_string(),
                );
            }
            _ => return false,
        }
        true
    }

    /// A webhook `by` makes in `channel` of `guild`, named `name`.
    fn make_hook(d: &mut Discord, channel: &str, guild: &str, by: &str, name: &str) -> Hook {
        let hook_id = d.id();
        let hook = Hook {
            token: format!("wtok-{hook_id}_x"),
            id: hook_id,
            channel: channel.to_owned(),
            guild: guild.to_owned(),
            by: by.to_owned(),
            name: name.to_owned(),
        };
        d.hooks.push(hook.clone());
        hook
    }

    /// A Discord server `guild` owned by `owner`, whose everyone role views channels, with a role
    /// that manages webhooks and one that does nothing, and `members`: each user's id, roles and
    /// nickname.
    fn server(d: &Shared, guild: &str, owner: &str, members: &[(&str, &[&str], Option<&str>)]) {
        let s = |bits: u64| json!(bits.to_string());
        let mut d = d.lock().expect("the stand-in");
        d.servers.insert(
            guild.to_owned(),
            Server {
                owner: owner.to_owned(),
                roles: vec![
                    json!({ "id": guild, "permissions": s(VIEW_CHANNEL) }),
                    json!({ "id": HOOKS_ROLE, "permissions": s(MANAGE_WEBHOOKS) }),
                    json!({ "id": PLAIN_ROLE, "permissions": s(0) }),
                ],
                members: members
                    .iter()
                    .map(|(user, roles, nick)| {
                        (
                            (*user).to_owned(),
                            json!({ "user": { "id": user }, "roles": roles, "nick": nick }),
                        )
                    })
                    .collect(),
            },
        );
    }

    /// A text channel `id` of `guild`, with the permission overwrites `overwrites`.
    fn text_channel(d: &Shared, guild: &str, id: &str, overwrites: Value) {
        d.lock().expect("the stand-in").chans.insert(
            id.to_owned(),
            Chan {
                guild: guild.to_owned(),
                kind: 0,
                overwrites,
                lines: Vec::new(),
            },
        );
    }

    /// `user` says `content` in the server channel `channel`, the line holding `extra` besides
    /// (its mentions, its attachments, a webhook's id, its kind): the line's id.
    fn channel_says(
        d: &Shared,
        channel: &str,
        user: &Value,
        content: &str,
        extra: &Value,
    ) -> String {
        let mut d = d.lock().expect("the stand-in");
        let id = d.id();
        let mut line = json!({
            "id": id, "type": 0, "channel_id": channel, "content": content, "author": user,
            "mentions": [], "attachments": [],
        });
        for (k, v) in extra.as_object().into_iter().flatten() {
            line[k] = v.clone();
        }
        d.chans
            .get_mut(channel)
            .expect("the channel")
            .lines
            .push(line);
        id
    }

    /// The lines said through webhooks in `channel`, once there are `n`.
    fn said_in_channel(d: &Shared, channel: &str, n: usize) -> Vec<Value> {
        within("the lines in the channel", || {
            let lines: Vec<Value> = d
                .lock()
                .expect("the stand-in")
                .channel_lines(channel)
                .into_iter()
                .filter(|l| !l["webhook_id"].is_null())
                .collect();
            (lines.len() >= n).then_some(lines)
        })
    }

    /// The contents of the lines said through webhooks in `channel`, now.
    fn contents_in(d: &Shared, channel: &str) -> Vec<String> {
        d.lock()
            .expect("the stand-in")
            .channel_lines(channel)
            .iter()
            .filter(|l| !l["webhook_id"].is_null())
            .map(|l| l["content"].as_str().unwrap_or("").to_owned())
            .collect()
    }

    /// A hub with sign-in and the broker on, reading direct messages every 50 ms.
    fn hub(stub: &Stub) -> Fixture {
        let broker = broker_for(stub);
        hub_with(move |c| c.broker = Some(broker))
    }

    /// Registers `slug` on the network: its key.
    fn register(f: &Fixture, slug: &str) -> String {
        let reg = Registration::from_json(&json!({
            "slug": slug, "name": format!("World {slug}"), "ruleset": "PvE",
            "endpoint": { "mode": "manual", "address": "192.0.2.1", "port": 9000 },
            "emulator": { "family": "ACE" },
        }))
        .expect("a registration");
        let r = f.running.registry();
        let (_, key) = r.register(&reg).expect("registered");
        r.set_federation(slug, Federation::On).expect("on");
        key
    }

    /// `slug`'s link, greeted, carrying `rooms` once the hub has taken its joins: a request sent after
    /// them on the same link is answered only after they are taken.
    fn link(f: &Fixture, slug: &str, key: &str, rooms: &[&str]) -> Link {
        link_at(&f.addr().to_string(), slug, key, rooms)
    }

    /// [`link`], to the hub at `addr`.
    fn link_at(addr: &str, slug: &str, key: &str, rooms: &[&str]) -> Link {
        let mut l = Link::open(&format!("ws://{addr}"), slug, key).expect("links");
        l.greet().expect("welcomed");
        for room in rooms {
            l.send(&Message::RoomJoin {
                room: (*room).into(),
                on: true,
            })
            .expect("sent");
        }
        let id = format!("{slug}-joined");
        l.send_raw(
            &json!({ "type": "channel", "id": id, "op": "list", "channel": "joined",
                     "by": format!("Sync@{slug}") })
            .to_string(),
        )
        .expect("sent");
        l.expect(WAIT, "the joins taken", |m| {
            matches!(m, Message::ChannelResult { id: i, .. } if *i == id).then_some(())
        })
        .expect("taken");
        l
    }

    /// A world registered and linked, carrying `rooms`.
    pub(super) fn world(f: &Fixture, slug: &str, rooms: &[&str]) -> Link {
        let key = register(f, slug);
        link(f, slug, &key, rooms)
    }

    pub(super) fn a(s: &str) -> Address {
        Address::parse(s).expect("an address")
    }

    /// `from` says `text` in `room` from its world's link: the hub's answer.
    fn say(l: &mut Link, id: &str, room: &str, from: &str, text: &str) -> Outcome {
        l.send(&Message::RoomSay {
            id: id.into(),
            room: room.into(),
            from: a(from),
            text: text.into(),
            via: None,
        })
        .expect("sent");
        l.expect(WAIT, "the room line's answer", |m| match m {
            Message::RoomResult { id: i, outcome } if i == id => Some(*outcome),
            _ => None,
        })
        .expect("an answer")
    }

    /// The next room line `l` is handed: its room, speaker and text.
    fn next_line(l: &mut Link) -> (String, String, String) {
        l.expect(WAIT, "a room line", |m| match m {
            Message::RoomLine {
                room, from, text, ..
            } => Some((room.clone(), from.to_string(), text.clone())),
            _ => None,
        })
        .expect("a room line")
    }

    /// What the admin route says of the mirrors and the tells.
    fn admin_view(f: &Fixture, admin: &str) -> Value {
        let r = op(f, admin, "GET", "/v1/admin/discord", &Value::Null);
        assert_eq!(r.status, 200, "{r:?}");
        r.json()
    }

    // ---- the admins' mirrors ----------------------------------------------------------------------

    /// The stand-in's server, its owner, and the channels the tests mirror rooms into.
    pub(super) const GUILD: &str = "1558376951000000000";
    /// See [`GUILD`].
    pub(super) const GUILD_OWNER: &str = "1558376953000000000";
    /// See [`GUILD`].
    pub(super) const CHANNEL: &str = "1558376951000000101";

    /// The stand-in's server, with the bot and the hub's admin in it, both managing webhooks, and
    /// `members`, and its channel [`CHANNEL`], open to everyone.
    pub(super) fn server_with_channel(d: &Shared, members: &[(&str, &[&str], Option<&str>)]) {
        let mut all: Vec<(&str, &[&str], Option<&str>)> =
            vec![(BOT, &[HOOKS_ROLE], None), (ADMIN, &[HOOKS_ROLE], None)];
        all.extend_from_slice(members);
        server(d, GUILD, GUILD_OWNER, &all);
        text_channel(d, GUILD, CHANNEL, json!([]));
    }

    /// The admin mirrors `room` as `body` asks: the admin route's answer.
    fn admin_mirrors(
        f: &Fixture,
        admin: &str,
        room: &str,
        body: &Value,
    ) -> super::super::operators::Answer {
        op(
            f,
            admin,
            "PUT",
            &format!("/v1/admin/discord/mirrors/{room}"),
            body,
        )
    }

    /// A hub with sign-in and the broker on, the admin signed in, mirroring the Global room into
    /// [`CHANNEL`]: the hub and the admin's session.
    fn hub_mirroring_global(stub: &Stub) -> (Fixture, String) {
        let f = hub(stub);
        let admin = sign_in(&f, ADMIN);
        let r = admin_mirrors(&f, &admin, "global", &json!({ "channel": CHANNEL }));
        assert_eq!(r.status, 200, "{r:?}");
        (f, admin)
    }

    /// The webhook the bot made in `channel`, once it has.
    fn hook_in(d: &Shared, channel: &str) -> Hook {
        within("the webhook", || {
            d.lock()
                .expect("the stand-in")
                .hooks
                .iter()
                .find(|h| h.channel == channel && h.by == BOT)
                .cloned()
        })
    }

    /// Registers each of `slugs` on the network of the hub `running`: their keys.
    fn register_on(running: &Running, slugs: &[&str]) -> Vec<String> {
        let r = running.registry();
        slugs
            .iter()
            .map(|slug| {
                let reg = Registration::from_json(&json!({
                    "slug": slug, "name": format!("World {slug}"), "ruleset": "PvE",
                    "endpoint": { "mode": "manual", "address": "192.0.2.1", "port": 9000 },
                    "emulator": { "family": "ACE" },
                }))
                .expect("a registration");
                let (_, key) = r.register(&reg).expect("registered");
                r.set_federation(slug, Federation::On).expect("on");
                key
            })
            .collect()
    }

    /// Spec: hub: a line said in a mirrored room is said in the mirror's channel through a webhook
    /// the bot makes there, and finds again after a restart rather than making another: shown under
    /// its speaker's address, with the picture the admins set, through the line rules with
    /// Discord's markdown escaped, calling nobody and previewing no link, with no credential but
    /// the webhook's own; a speaker whose address Discord refuses as a name is named in the line
    /// instead; the worlds are handed it as before.
    #[test]
    fn a_line_goes_into_a_server_channel_through_the_bots_webhook_under_its_speakers_address() {
        let (stub, d) = stand_in();
        server_with_channel(&d, &[]);
        let prepare = |r: &Registry| {
            r.set_mirror(None, "global", &Placement::bot(CHANNEL, GUILD), None)
                .expect("mirrored");
            r.set_discord_avatar(Some("https://hub.test/dereth.png"), None)
                .expect("set");
        };
        let running = hub_on(":memory:", broker_for(&stub), prepare);
        let keys = register_on(&running, &["aa", "bb"]);
        let hook = hook_in(&d, CHANNEL);
        assert_eq!(
            (hook.by.as_str(), hook.name.as_str()),
            (BOT, "Dereth Network"),
            "the bot's own"
        );
        let addr = running.local_addr().to_string();
        let mut aa = link_at(&addr, "aa", &keys[0], &["global"]);
        let mut bb = link_at(&addr, "bb", &keys[1], &["global"]);
        let text = "Raid at <Tell:IIDString:1:Asheron>Asheron<\\Tell>'s, @everyone: **now** <@80351110224678912>";
        assert_eq!(say(&mut aa, "r1", "global", "Ann@aa", text), Outcome::Ok);
        assert_eq!(
            next_line(&mut bb),
            ("global".into(), "Ann@aa".into(), text.into()),
            "the worlds are handed it as before"
        );
        let lines = said_in_channel(&d, CHANNEL, 1);
        assert_eq!(lines[0]["author"]["username"], "Ann@aa");
        assert_eq!(
            lines[0]["content"],
            "Raid at Asheron's, @\u{200B}everyone: \\*\\*now\\*\\* \\<\u{200B}@80351110224678912\\>"
        );
        let sent = d.lock().expect("the stand-in").webhook_calls();
        assert_eq!(
            sent[0].path,
            format!("/api/webhooks/{}/{}?wait=true", hook.id, hook.token)
        );
        assert_eq!(sent[0].auth, "", "no credential but the webhook's own");
        assert_eq!(
            sent[0].body["allowed_mentions"],
            json!({ "parse": [] }),
            "calls nobody"
        );
        assert_eq!(sent[0].body["flags"], 4, "previews no link");
        assert_eq!(sent[0].body["avatar_url"], "https://hub.test/dereth.png");
        assert_eq!(
            say(&mut aa, "r2", "global", "Clyde@aa", "hello"),
            Outcome::Ok
        );
        let lines = said_in_channel(&d, CHANNEL, 2);
        assert_eq!(lines[1]["author"]["username"], "Dereth Network");
        assert_eq!(lines[1]["content"], "Clyde@aa: hello");
        {
            let d = d.lock().expect("the stand-in");
            assert!(d.webhook_calls()[1].body.get("username").is_none());
            assert_eq!(d.count("POST", "/api/channels/"), 1, "one webhook made");
        }
        aa.close();
        bb.close();
        running.shutdown();

        let looked = d
            .lock()
            .expect("the stand-in")
            .count("GET", &format!("/api/channels/{CHANNEL}/webhooks"));
        let running = hub_on(":memory:", broker_for(&stub), prepare);
        within("the webhook looked for again", || {
            let d = d.lock().expect("the stand-in");
            (d.count("GET", &format!("/api/channels/{CHANNEL}/webhooks")) > looked).then_some(())
        });
        let keys = register_on(&running, &["aa"]);
        let mut aa = link_at(
            &running.local_addr().to_string(),
            "aa",
            &keys[0],
            &["global"],
        );
        assert_eq!(
            say(&mut aa, "r3", "global", "Ann@aa", "after a restart"),
            Outcome::Ok
        );
        let lines = said_in_channel(&d, CHANNEL, 3);
        assert_eq!(lines[2]["webhook_id"], hook.id.as_str());
        assert_eq!(
            d.lock()
                .expect("the stand-in")
                .count("POST", "/api/channels/"),
            1,
            "found again, not made again"
        );
        aa.close();
        running.shutdown();
    }

    /// Spec: hub: a line into a mirror waits out a short wait Discord names for the channel's
    /// webhook (a `429`, or an answer saying the webhook has no call left) and is held back past a
    /// longer one, counted, while Discord is asked nothing on the webhook.
    #[test]
    fn discords_waits_on_a_webhook_are_kept() {
        let (stub, d) = stand_in();
        server_with_channel(&d, &[]);
        let (f, admin) = hub_mirroring_global(&stub);
        hook_in(&d, CHANNEL);
        let mut aa = world(&f, "aa", &["global"]);
        let mut bb = world(&f, "bb", &["global"]);
        let hook = d.lock().expect("the stand-in").hook_path(CHANNEL);
        // Each line by another speaker, within the room's pace for one.
        let say_it = |aa: &mut Link, bb: &mut Link, id: &str, from: &str, text: &str| {
            assert_eq!(say(aa, id, "global", from, text), Outcome::Ok);
            assert_eq!(next_line(bb).2, text, "every world is handed it");
        };

        // A 429 of a second: the line waits it out.
        d.lock()
            .expect("the stand-in")
            .busy
            .insert(hook.clone(), (1, 1));
        say_it(&mut aa, &mut bb, "r1", "Ann@aa", "first");
        assert_eq!(said_in_channel(&d, CHANNEL, 1)[0]["content"], "first");
        let (refused_at, sent_at) = {
            let d = d.lock().expect("the stand-in");
            let refused = d.busied.iter().find(|(p, _)| *p == hook).expect("a 429").1;
            (refused, d.webhook_calls().last().expect("said").at)
        };
        assert!(sent_at.duration_since(refused_at) >= Duration::from_millis(900));

        // An answer saying the webhook has no call left for a second: the next line waits for it,
        // and is never refused.
        d.lock()
            .expect("the stand-in")
            .spent
            .insert(hook.clone(), 1.0);
        say_it(&mut aa, &mut bb, "r2", "Bob@aa", "second");
        said_in_channel(&d, CHANNEL, 2);
        say_it(&mut aa, &mut bb, "r3", "Cy@aa", "third");
        said_in_channel(&d, CHANNEL, 3);
        {
            let d = d.lock().expect("the stand-in");
            let calls = d.webhook_calls();
            let (second, third) = (&calls[calls.len() - 2], &calls[calls.len() - 1]);
            assert!(third.at.duration_since(second.at) >= Duration::from_millis(900));
            assert_eq!(d.busied.len(), 1, "no 429 but the first");
        }

        // A 429 of half a minute: the line is held back and counted, and the next is held back
        // without Discord being asked.
        d.lock()
            .expect("the stand-in")
            .busy
            .insert(hook.clone(), (30, 1));
        say_it(&mut aa, &mut bb, "r4", "Di@aa", "fourth");
        let held = |n: u64| {
            within("the lines held back", || {
                let v = admin_view(&f, &admin);
                (v["mirrors"][0]["lines"]["heldBackToDiscord"] == n).then_some(v)
            })
        };
        held(1);
        let asked = d.lock().expect("the stand-in").webhook_calls().len();
        say_it(&mut aa, &mut bb, "r5", "Ed@aa", "fifth");
        let view = held(2);
        assert_eq!(
            d.lock().expect("the stand-in").webhook_calls().len(),
            asked,
            "Discord is not asked within the wait"
        );
        assert_eq!(
            view["mirrors"][0]["lines"],
            json!({ "toDiscord": 3, "heldBackToDiscord": 2, "notCarried": 0 })
        );
        aa.close();
        bb.close();
    }

    /// Spec: hub: a line said in the game goes to every mirror of its room, the admins' and each
    /// world operator's of the Global room, each into its own server's channel; no mirror's
    /// channel is ever read, so nothing said in Discord reaches the worlds.
    #[test]
    fn a_line_said_in_the_game_goes_to_every_mirror_of_its_room_and_nothing_comes_back() {
        let (stub, d) = stand_in();
        let (owner_a, owner_b) = ("80351110224678920", "80351110224678921");
        let (guild_a, guild_b) = ("1558376951000000500", "1558376951000000600");
        let (chan_a, chan_b) = ("1558376951000000501", "1558376951000000601");
        server_with_channel(&d, &[]);
        server(
            &d,
            guild_a,
            owner_a,
            &[(BOT, &[HOOKS_ROLE], None), (owner_a, &[], None)],
        );
        server(
            &d,
            guild_b,
            owner_b,
            &[(BOT, &[HOOKS_ROLE], None), (owner_b, &[], None)],
        );
        text_channel(&d, guild_a, chan_a, json!([]));
        text_channel(&d, guild_b, chan_b, json!([]));
        let (f, _admin) = hub_mirroring_global(&stub);
        let key_a = register(&f, "aa");
        let key_b = register(&f, "bb");
        let (session_a, session_b) = (sign_in(&f, owner_a), sign_in(&f, owner_b));
        {
            let r = f.running.registry();
            r.assign("aa", owner_a).expect("handed to its operator");
            r.assign("bb", owner_b).expect("handed to its operator");
        }
        for (session, slug, channel) in [(&session_a, "aa", chan_a), (&session_b, "bb", chan_b)] {
            let r = op(
                &f,
                session,
                "PUT",
                &format!("/v1/operator/worlds/{slug}/discord/mirrors/global"),
                &json!({ "channel": channel }),
            );
            assert_eq!(r.status, 200, "{r:?}");
        }
        for c in [CHANNEL, chan_a, chan_b] {
            hook_in(&d, c);
        }
        let mut aa = link(&f, "aa", &key_a, &["global"]);
        let mut bb = link(&f, "bb", &key_b, &["global"]);
        assert_eq!(
            say(&mut aa, "r1", "global", "Ann@aa", "from the game"),
            Outcome::Ok
        );
        assert_eq!(next_line(&mut bb).2, "from the game");
        for c in [CHANNEL, chan_a, chan_b] {
            let lines = said_in_channel(&d, c, 1);
            assert_eq!(
                (&lines[0]["author"]["username"], &lines[0]["content"]),
                (&json!("Ann@aa"), &json!("from the game")),
                "{c}"
            );
        }
        let nelly = discord_user("80351110224678912", "Nelly", "nelly");
        for c in [CHANNEL, chan_a, chan_b] {
            channel_says(&d, c, &nelly, "said in Discord", &json!({}));
        }
        for l in [&mut aa, &mut bb] {
            l.quiet(Duration::from_millis(800), "a room line", |m| {
                matches!(m, Message::RoomLine { .. })
            })
            .expect("nothing said in Discord reaches the game");
        }
        for c in [CHANNEL, chan_a, chan_b] {
            assert_eq!(
                d.lock().expect("the stand-in").channel_reads(c),
                0,
                "{c} is never read"
            );
            assert_eq!(
                contents_in(&d, c),
                ["from the game"],
                "{c}: nothing relayed"
            );
        }
        aa.close();
        bb.close();
    }

    /// Spec: hub: the admins mirror the Global room and network channels through their routes,
    /// each change in the hub's own Discord log under the admin who made it, and set the picture
    /// the lines show, live; a channel mirrors one room; a mirror taken away carries nothing more;
    /// and none of it is any other operator's to do.
    #[test]
    fn the_admins_set_their_own_mirrors_and_the_picture_live() {
        let (stub, d) = stand_in();
        let second = "1558376951000000102";
        server_with_channel(&d, &[]);
        text_channel(&d, GUILD, second, json!([]));
        let (f, admin) = hub_mirroring_global(&stub);
        let operator = sign_in(&f, "80351110224678920");
        assert_eq!(
            admin_mirrors(&f, &operator, "global", &json!({ "channel": second })).status,
            403,
            "admins only"
        );
        let taken = admin_mirrors(
            &f,
            &admin,
            "oldguard%40channel",
            &json!({ "channel": CHANNEL }),
        );
        assert_eq!(
            (taken.status, taken.reason().as_str()),
            (409, "channel_taken")
        );
        assert_eq!(
            admin_mirrors(&f, &admin, "trade", &json!({ "channel": second })).status,
            400,
            "not one of the network's rooms or channels"
        );
        let set = admin_mirrors(
            &f,
            &admin,
            "oldguard%40channel",
            &json!({ "channel": second }),
        );
        assert_eq!(set.status, 200, "{set:?}");
        let mirrors = set.json()["mirrors"].clone();
        assert_eq!(
            mirrors
                .as_array()
                .expect("a list")
                .iter()
                .map(|m| (m["world"].clone(), m["room"].clone(), m["channel"].clone()))
                .collect::<Vec<_>>(),
            [
                (Value::Null, json!("global"), json!(CHANNEL)),
                (Value::Null, json!("oldguard@channel"), json!(second)),
            ]
        );
        let me =
            op(&f, &admin, "GET", "/v1/operator", &Value::Null).json()["operator"]["id"].clone();
        let log = op(&f, &admin, "GET", "/v1/admin/discord/log", &Value::Null).json();
        assert_eq!(log["events"][0]["kind"], "discord-mirror");
        assert_eq!(log["events"][0]["actor"], me);
        assert_eq!(
            log["events"][0]["details"],
            json!({ "room": "oldguard@channel", "mirrored": true, "channel": second,
                    "server": GUILD, "through": "bot" })
        );

        hook_in(&d, CHANNEL);
        let mut aa = world(&f, "aa", &["global"]);
        assert_eq!(say(&mut aa, "r1", "global", "Ann@aa", "plain"), Outcome::Ok);
        said_in_channel(&d, CHANNEL, 1);
        assert!(d.lock().expect("the stand-in").webhook_calls()[0]
            .body
            .get("avatar_url")
            .is_none());
        let bad = op(
            &f,
            &admin,
            "PUT",
            "/v1/admin/discord/avatar",
            &json!({ "url": "http://hub.test/a.png" }),
        );
        assert_eq!(bad.status, 400);
        let pic = op(
            &f,
            &admin,
            "PUT",
            "/v1/admin/discord/avatar",
            &json!({ "url": "https://hub.test/dereth.png" }),
        );
        assert_eq!(pic.json()["avatarUrl"], "https://hub.test/dereth.png");
        assert_eq!(
            say(&mut aa, "r2", "global", "Bob@aa", "pictured"),
            Outcome::Ok
        );
        said_in_channel(&d, CHANNEL, 2);
        assert_eq!(
            d.lock().expect("the stand-in").webhook_calls()[1].body["avatar_url"],
            "https://hub.test/dereth.png",
            "live"
        );
        assert_eq!(
            op(&f, &admin, "GET", "/v1/admin/discord/log", &Value::Null).json()["events"][0]
                ["details"],
            json!({ "url": "https://hub.test/dereth.png" })
        );

        assert_eq!(
            op(
                &f,
                &admin,
                "DELETE",
                "/v1/admin/discord/mirrors/global",
                &Value::Null
            )
            .status,
            204
        );
        assert_eq!(
            op(
                &f,
                &admin,
                "DELETE",
                "/v1/admin/discord/mirrors/global",
                &Value::Null
            )
            .status,
            404
        );
        assert_eq!(
            say(&mut aa, "r3", "global", "Cy@aa", "unmirrored"),
            Outcome::Ok
        );
        std::thread::sleep(Duration::from_millis(500));
        assert_eq!(
            contents_in(&d, CHANNEL),
            ["plain", "pictured"],
            "carried there no more"
        );
        aa.close();
    }

    /// Spec: hub: a channel mirrored into Discord is said in its mirror when a member tells it,
    /// under the member's address.
    #[test]
    fn a_mirrored_channel_takes_its_members_lines() {
        let (stub, d) = stand_in();
        server_with_channel(&d, &[]);
        let f = hub(&stub);
        let admin = sign_in(&f, ADMIN);
        let set = admin_mirrors(
            &f,
            &admin,
            "oldguard@channel",
            &json!({ "channel": CHANNEL }),
        );
        assert_eq!(set.status, 200, "{set:?}");
        hook_in(&d, CHANNEL);
        let mut aa = world(&f, "aa", &[]);
        let mut bb = world(&f, "bb", &[]);
        let channel = |l: &mut Link, id: &str, op: &str, by: &str, who: Option<&str>| {
            let mut m =
                json!({ "type": "channel", "id": id, "op": op, "channel": "oldguard", "by": by });
            if let Some(who) = who {
                m["who"] = json!(who);
            }
            l.send_raw(&m.to_string()).expect("sent");
            l.expect(WAIT, "the channel's answer", |m| match m {
                Message::ChannelResult { id: i, outcome, .. } if i == id => Some(*outcome),
                _ => None,
            })
            .expect("an answer")
        };
        assert_eq!(
            channel(&mut aa, "c1", "create", "Ann@aa", None),
            ChannelOutcome::Ok
        );
        assert_eq!(
            channel(&mut aa, "c2", "invite", "Ann@aa", Some("Bob@bb")),
            ChannelOutcome::Ok
        );
        assert_eq!(
            channel(&mut bb, "c3", "join", "Bob@bb", None),
            ChannelOutcome::Ok
        );
        aa.send(&Message::Tell {
            id: "t1".into(),
            from: a("Ann@aa"),
            to: a("oldguard@channel"),
            text: "raid at 9".into(),
            friend: false,
        })
        .expect("sent");
        let lines = said_in_channel(&d, CHANNEL, 1);
        assert_eq!(
            (&lines[0]["author"]["username"], &lines[0]["content"]),
            (&json!("Ann@aa"), &json!("raid at 9"))
        );
        aa.close();
        bb.close();
    }

    // ---- a world's own mirrors ------------------------------------------------------------------

    /// Spec: hub: a world's room is mirrored only at its operator's asking, signed in, and never
    /// another operator's: refused, saying why, where the bot cannot see the channel or lacks a
    /// permission it needs there, where the operator may not manage the channel's webhooks, and
    /// where the channel mirrors another room; the invite asks the bot no permission; once set, the
    /// mirror is in the world's log and carries the room's lines for that world alone, every one
    /// of the world's four rooms and the Global room may be mirrored and no other, and once it is
    /// taken away a line in the world's room is refused; the world's key mirrors nothing.
    #[test]
    fn a_worlds_room_goes_into_a_server_channel_only_at_its_operators_asking_where_both_may() {
        let (stub, d) = stand_in();
        let (owner, stranger) = ("80351110224678920", "80351110224678921");
        let (ours, hidden, no_hooks, hubs) = (
            "1558376951000000101",
            "1558376951000000102",
            "1558376951000000103",
            "1558376951000000104",
        );
        let (other_guild, not_theirs, nowhere) = (
            "1558376951000000500",
            "1558376951000000501",
            "1558376951000000999",
        );
        server(
            &d,
            GUILD,
            GUILD_OWNER,
            &[
                (BOT, &[HOOKS_ROLE], None),
                (ADMIN, &[HOOKS_ROLE], None),
                (owner, &[HOOKS_ROLE], None),
                (stranger, &[], None),
            ],
        );
        server(&d, other_guild, GUILD_OWNER, &[(BOT, &[HOOKS_ROLE], None)]);
        let bits = |b: u64| json!(b.to_string());
        text_channel(&d, GUILD, ours, json!([]));
        text_channel(
            &d,
            GUILD,
            hidden,
            json!([
                { "id": GUILD, "type": 0, "allow": "0", "deny": bits(VIEW_CHANNEL) },
                { "id": owner, "type": 1, "allow": bits(VIEW_CHANNEL), "deny": "0" },
            ]),
        );
        text_channel(
            &d,
            GUILD,
            no_hooks,
            json!([{ "id": BOT, "type": 1, "allow": "0", "deny": bits(MANAGE_WEBHOOKS) }]),
        );
        text_channel(&d, GUILD, hubs, json!([]));
        text_channel(&d, other_guild, not_theirs, json!([]));
        let f = hub(&stub);
        let admin = sign_in(&f, ADMIN);
        assert_eq!(
            admin_mirrors(&f, &admin, "global", &json!({ "channel": hubs })).status,
            200
        );
        let owner_session = sign_in(&f, owner);
        let stranger_session = sign_in(&f, stranger);
        let key = register(&f, "aa");
        let other_key = register(&f, "bb");
        f.running
            .registry()
            .assign("aa", owner)
            .expect("handed to its operator");
        let route = "/v1/operator/worlds/aa/discord/mirrors";
        let put = |session: &str, room: &str, body: Value| {
            op(&f, session, "PUT", &format!("{route}/{room}"), &body)
        };
        let into = |channel: &str| json!({ "channel": channel });
        assert_eq!(
            put(&stranger_session, "general", into(ours)).status,
            403,
            "another operator"
        );
        assert_eq!(
            call(
                f.addr(),
                "PUT",
                "/v1/servers/aa/discord/rooms/general",
                &[
                    ("Authorization", &format!("Bearer {key}")),
                    ("Content-Type", "application/json"),
                ],
                &into(ours).to_string(),
            )
            .status,
            404,
            "the world's key mirrors nothing"
        );
        for (channel, status, reason) in [
            (hidden, 409, "bot_cannot_see"),
            (nowhere, 409, "bot_cannot_see"),
            (no_hooks, 409, "bot_lacks_permissions"),
            (not_theirs, 403, "not_your_channel"),
            (hubs, 409, "channel_taken"),
        ] {
            let r = put(&owner_session, "general", into(channel));
            assert_eq!((r.status, r.reason().as_str()), (status, reason), "{r:?}");
        }
        assert!(
            put(&owner_session, "general", into(no_hooks)).json()["error"]["message"]
                .as_str()
                .is_some_and(|m| m.contains("Manage Webhooks")),
            "names what the bot lacks"
        );
        assert_eq!(
            put(&owner_session, "society", into(ours)).status,
            400,
            "not one of the world's rooms"
        );
        let view = op(&f, &owner_session, "GET", route, &Value::Null).json();
        assert_eq!(view["mirrors"], json!([]), "nothing refused was kept");
        assert_eq!(
            view["rooms"],
            json!(["global", "general", "trade", "lfg", "roleplay"])
        );
        assert!(view["invite"].as_str().is_some_and(
            |i| i.ends_with("?client_id=app&scope=bot%20applications.commands&permissions=0")
        ));

        let set = put(&owner_session, "general", into(ours));
        assert_eq!(set.status, 200, "{set:?}");
        let mirror = &set.json()["mirrors"][0];
        assert_eq!(
            (
                &mirror["room"],
                &mirror["channel"],
                &mirror["server"],
                &mirror["through"]
            ),
            (
                &json!("general"),
                &json!(ours),
                &json!(GUILD),
                &json!("bot")
            )
        );
        let audit = f.running.registry().audit("aa").expect("the log");
        assert_eq!(audit[0]["kind"], "discord-mirror");
        assert_eq!(
            audit[0]["details"],
            json!({ "room": "general", "mirrored": true, "channel": ours, "server": GUILD,
                    "through": "bot" })
        );

        let hook = hook_in(&d, ours);
        let mut aa = link(&f, "aa", &key, &["general"]);
        let mut bb = link(&f, "bb", &other_key, &["general"]);
        assert_eq!(
            say(&mut aa, "r1", "general", "Ann@aa", "local news"),
            Outcome::Ok
        );
        let lines = said_in_channel(&d, ours, 1);
        assert_eq!(
            (&lines[0]["author"]["username"], &lines[0]["content"]),
            (&json!("Ann@aa"), &json!("local news"))
        );
        assert_eq!(lines[0]["webhook_id"], hook.id.as_str());
        assert_eq!(
            say(&mut bb, "r2", "general", "Bob@bb", "not mirrored here"),
            Outcome::NotMember,
            "bb's own room of that name is not mirrored"
        );

        for room in ["trade", "lfg", "roleplay"] {
            let channel = d.lock().expect("the stand-in").id();
            text_channel(&d, GUILD, &channel, json!([]));
            assert_eq!(
                put(&owner_session, room, into(&channel)).status,
                200,
                "{room}: no cap"
            );
        }
        let all = op(&f, &owner_session, "GET", route, &Value::Null).json();
        assert_eq!(all["mirrors"].as_array().map(Vec::len), Some(4));

        assert_eq!(
            op(
                &f,
                &owner_session,
                "DELETE",
                &format!("{route}/general"),
                &Value::Null
            )
            .status,
            204
        );
        assert_eq!(
            say(&mut aa, "r3", "general", "Ann@aa", "after"),
            Outcome::NotMember
        );
        assert_eq!(
            op(
                &f,
                &owner_session,
                "DELETE",
                &format!("{route}/general"),
                &Value::Null
            )
            .status,
            404
        );
        aa.close();
        bb.close();
    }

    /// The next rooms message `l` is handed that `pick` takes: where it carries the Global room,
    /// and its own rooms mirrored.
    fn rooms_of(
        l: &mut Link,
        what: &str,
        pick: impl Fn(Option<&str>) -> bool,
    ) -> (Option<String>, Vec<String>) {
        l.expect_rooms(WAIT, what, |m| match m {
            RoomsMessage::Setup {
                id,
                global,
                mirrored,
            } if pick(id.as_deref()) => Some((global.clone(), mirrored.clone())),
            _ => None,
        })
        .expect("its rooms")
    }

    /// `l`'s world asks to carry the Global room in `room`, or seeds it: the hub's answer.
    fn ask_global(l: &mut Link, id: &str, room: &str, seed: bool) -> (Option<String>, Vec<String>) {
        l.send_raw(
            &RoomsMessage::Global {
                id: id.into(),
                room: room.into(),
                seed,
            }
            .to_json(),
        )
        .expect("sent");
        rooms_of(l, "the answer", |i| i == Some(id))
    }

    fn rooms(global: Option<&str>, mirrored: &[&str]) -> (Option<String>, Vec<String>) {
        (
            global.map(str::to_owned),
            mirrored.iter().map(|r| (*r).to_owned()).collect(),
        )
    }

    /// Spec: hub: a world is told its rooms at its welcome and whenever they change; it seeds the
    /// room that carries its Global room while the hub holds none, and the hub keeps the seed once
    /// and ignores the next; when the room becomes known, the world's mirror of it carries the
    /// Global room into the same channel, as the hub records; that room is refused as an own
    /// room's mirror; the world's operator sets it on the site and the world itself sets it, each
    /// told to the world at once, and a room's mirror goes where Global is mirrored already.
    #[test]
    fn a_worlds_mirror_of_the_room_carrying_global_carries_the_global_room() {
        let (stub, d) = stand_in();
        let owner = "80351110224678920";
        let (general, roleplay, trade) = (
            "1558376951000000101",
            "1558376951000000102",
            "1558376951000000103",
        );
        server(
            &d,
            GUILD,
            GUILD_OWNER,
            &[(BOT, &[HOOKS_ROLE], None), (owner, &[HOOKS_ROLE], None)],
        );
        for channel in [general, roleplay, trade] {
            text_channel(&d, GUILD, channel, json!([]));
        }
        let f = hub(&stub);
        let session = sign_in(&f, owner);
        let key = register(&f, "aa");
        f.running
            .registry()
            .assign("aa", owner)
            .expect("handed to its operator");
        let mirrors = "/v1/operator/worlds/aa/discord/mirrors";
        let into = |room: &str, channel: &str| {
            op(
                &f,
                &session,
                "PUT",
                &format!("{mirrors}/{room}"),
                &json!({ "channel": channel }),
            )
        };
        // As the owner had it: Roleplay mirrored, before the hub knew Roleplay carries Global.
        assert_eq!(into("roleplay", roleplay).status, 200);
        assert_eq!(into("general", general).status, 200);
        hook_in(&d, roleplay);

        let mut aa = link(&f, "aa", &key, &["global", "general"]);
        assert_eq!(
            rooms_of(&mut aa, "its rooms at its welcome", |i| i.is_none()),
            rooms(None, &["general", "roleplay"]),
            "no Global room held yet"
        );
        assert_eq!(
            ask_global(&mut aa, "g1", "roleplay", true),
            rooms(Some("roleplay"), &["general"]),
            "the seed is kept"
        );
        let view = op(&f, &session, "GET", mirrors, &Value::Null).json();
        assert_eq!(view["globalRoom"], "roleplay");
        assert_eq!(
            view["rooms"],
            json!(["global", "general", "trade", "lfg"]),
            "Roleplay may not be mirrored as an own room"
        );
        let by_room: BTreeMap<String, String> = view["mirrors"]
            .as_array()
            .expect("mirrors")
            .iter()
            .map(|m| {
                (
                    m["room"].as_str().unwrap_or("").to_owned(),
                    m["channel"].as_str().unwrap_or("").to_owned(),
                )
            })
            .collect();
        assert_eq!(
            by_room,
            [
                ("general".to_owned(), general.to_owned()),
                ("global".to_owned(), roleplay.to_owned())
            ]
            .into(),
            "the Roleplay mirror carries Global into its channel"
        );
        let audit = op(&f, &session, "GET", "/v1/operator/worlds/aa", &Value::Null).json();
        let audit = &audit["audit"];
        assert_eq!(
            (
                &audit[0]["kind"],
                &audit[0]["actor"],
                &audit[0]["details"]["was"]
            ),
            (&json!("discord-mirror"), &json!("hub"), &json!("roleplay"))
        );
        assert_eq!(
            (&audit[1]["kind"], &audit[1]["actor"], &audit[1]["details"]),
            (
                &json!("global-room"),
                &json!("world"),
                &json!({ "room": "roleplay", "was": null, "seed": true })
            )
        );
        assert_eq!(
            say(&mut aa, "r1", "global", "Ann@aa", "at last"),
            Outcome::Ok
        );
        let said = within("the Global line in the Roleplay mirror's channel", || {
            Some(contents_in(&d, roleplay)).filter(|c| !c.is_empty())
        });
        assert_eq!(said, ["at last"]);
        let refused = into("roleplay", trade);
        assert_eq!(
            (refused.status, refused.reason().as_str()),
            (409, "carries_global"),
            "{refused:?}"
        );

        // The site sets it, and the world is told at once.
        let global_room = "/v1/operator/worlds/aa/global-room";
        let set = op(
            &f,
            &session,
            "PUT",
            global_room,
            &json!({ "room": "trade" }),
        );
        assert_eq!(
            (set.status, set.json()),
            (200, json!({ "globalRoom": "trade" }))
        );
        assert_eq!(
            rooms_of(&mut aa, "its rooms after the site set them", |i| i
                .is_none()),
            rooms(Some("trade"), &["general"])
        );
        assert_eq!(
            ask_global(&mut aa, "g2", "general", true),
            rooms(Some("trade"), &["general"]),
            "a seed while one is held changes nothing"
        );
        assert_eq!(
            op(&f, &session, "GET", global_room, &Value::Null).json(),
            json!({ "globalRoom": "trade" })
        );
        for bad in [
            json!({ "room": "global" }),
            json!({ "room": "Trade" }),
            json!({}),
        ] {
            assert_eq!(
                op(&f, &session, "PUT", global_room, &bad).status,
                400,
                "{bad}"
            );
        }
        let stranger = sign_in(&f, "80351110224678921");
        assert_eq!(
            op(&f, &stranger, "PUT", global_room, &json!({ "room": "lfg" })).status,
            403
        );

        // The world sets it itself: General's mirror goes, since Global is mirrored already.
        assert_eq!(
            ask_global(&mut aa, "g3", "general", false),
            rooms(Some("general"), &[])
        );
        let view = op(&f, &session, "GET", mirrors, &Value::Null).json();
        assert_eq!(
            view["mirrors"]
                .as_array()
                .expect("mirrors")
                .iter()
                .map(|m| m["room"].clone())
                .collect::<Vec<_>>(),
            [json!("global")]
        );
        let audit = op(&f, &session, "GET", "/v1/operator/worlds/aa", &Value::Null).json();
        assert_eq!(
            (&audit["audit"][0]["details"], &audit["audit"][1]["actor"]),
            (
                &json!({ "room": "general", "mirrored": false, "carriesGlobal": true }),
                &json!("world")
            )
        );
        aa.close();
    }

    /// Spec: hub: the admins see every world operator's mirror, and take any of them away, each
    /// change in the world's log, where the world's owner reads it as done by the hub's admins;
    /// the room is mirrored there no more.
    #[test]
    fn the_admins_take_away_a_worlds_global_mirror() {
        let (stub, d) = stand_in();
        let owner = "80351110224678920";
        let (guild, channel) = ("1558376951000000500", "1558376951000000501");
        server_with_channel(&d, &[]);
        server(
            &d,
            guild,
            owner,
            &[(BOT, &[HOOKS_ROLE], None), (owner, &[], None)],
        );
        text_channel(&d, guild, channel, json!([]));
        let (f, admin) = hub_mirroring_global(&stub);
        let key = register(&f, "aa");
        let session = sign_in(&f, owner);
        f.running
            .registry()
            .assign("aa", owner)
            .expect("handed to its operator");
        let route = "/v1/operator/worlds/aa/discord/mirrors";
        let set = op(
            &f,
            &session,
            "PUT",
            &format!("{route}/global"),
            &json!({ "channel": channel }),
        );
        assert_eq!(set.status, 200, "{set:?}");
        let mirrors = admin_view(&f, &admin)["mirrors"].clone();
        assert_eq!(
            (
                &mirrors[1]["world"],
                &mirrors[1]["room"],
                &mirrors[1]["channel"]
            ),
            (&json!("aa"), &json!("global"), &json!(channel))
        );
        hook_in(&d, channel);
        let mut aa = link(&f, "aa", &key, &["global"]);
        assert_eq!(
            say(&mut aa, "r1", "global", "Ann@aa", "before"),
            Outcome::Ok
        );
        said_in_channel(&d, channel, 1);

        let take = |session: &str| {
            op(
                &f,
                session,
                "DELETE",
                "/v1/admin/discord/worlds/aa/mirrors/global",
                &Value::Null,
            )
        };
        assert_eq!(take(&session).status, 403, "admins only");
        assert_eq!(take(&admin).status, 204);
        assert_eq!(take(&admin).status, 404, "taken already");
        assert_eq!(
            op(&f, &session, "GET", route, &Value::Null).json()["mirrors"],
            json!([])
        );
        let owners = op(&f, &session, "GET", "/v1/operator/worlds/aa", &Value::Null).json();
        assert_eq!(owners["audit"][0]["kind"], "discord-mirror");
        assert_eq!(
            owners["audit"][0]["details"],
            json!({ "room": "global", "mirrored": false })
        );
        assert_eq!(
            owners["audit"][0]["actor"], "hub-admin",
            "done by the hub's admins"
        );
        assert_eq!(say(&mut aa, "r2", "global", "Bob@aa", "after"), Outcome::Ok);
        said_in_channel(&d, CHANNEL, 2);
        assert_eq!(contents_in(&d, channel), ["before"], "taken away");
        aa.close();
    }

    /// Spec: hub: a mirror may go through a webhook its operator made in their own channel, where
    /// the bot is not: the hub reads the webhook once with its token, from Discord's own API, and
    /// says the room's lines through it; no answer, log event or view holds the webhook's token or
    /// address; an address that is not a Discord webhook's, one Discord does not have, and a
    /// channel that mirrors another room are refused.
    #[test]
    fn a_mirror_through_a_webhook_its_operator_made_keeps_the_webhook_secret() {
        let (stub, d) = stand_in();
        let owner = "80351110224678920";
        let (guild, channel) = ("1558376951000000700", "1558376951000000701");
        server_with_channel(&d, &[]);
        server(&d, guild, owner, &[(owner, &[HOOKS_ROLE], None)]);
        text_channel(&d, guild, channel, json!([]));
        let theirs = make_hook(
            &mut d.lock().expect("the stand-in"),
            channel,
            guild,
            owner,
            "Spidey Bot",
        );
        let taken_hook = make_hook(
            &mut d.lock().expect("the stand-in"),
            CHANNEL,
            GUILD,
            ADMIN,
            "Captain Hook",
        );
        let address = |h: &Hook| format!("https://discord.com/api/webhooks/{}/{}", h.id, h.token);
        let (f, admin) = hub_mirroring_global(&stub);
        let key = register(&f, "aa");
        let session = sign_in(&f, owner);
        f.running
            .registry()
            .assign("aa", owner)
            .expect("handed to its operator");
        let route = "/v1/operator/worlds/aa/discord/mirrors";
        let put = |body: Value| op(&f, &session, "PUT", &format!("{route}/trade"), &body);
        let r = put(json!({ "webhook": "https://evil.test/api/webhooks/1/x" }));
        assert_eq!((r.status, r.reason().as_str()), (400, "not_a_webhook"));
        let r = put(
            json!({ "webhook": format!("https://discord.com/api/webhooks/{}/wrong", theirs.id) }),
        );
        assert_eq!((r.status, r.reason().as_str()), (409, "unknown_webhook"));
        let r = put(json!({ "webhook": address(&taken_hook) }));
        assert_eq!((r.status, r.reason().as_str()), (409, "channel_taken"));
        let set = put(json!({ "webhook": address(&theirs) }));
        assert_eq!(set.status, 200, "{set:?}");
        let mirror = &set.json()["mirrors"][0];
        assert_eq!(
            (
                &mirror["room"],
                &mirror["channel"],
                &mirror["server"],
                &mirror["through"],
                &mirror["webhook"]
            ),
            (
                &json!("trade"),
                &json!(channel),
                &json!(guild),
                &json!("webhook"),
                &json!(theirs.id)
            )
        );
        let read = d
            .lock()
            .expect("the stand-in")
            .calls
            .iter()
            .filter(|c| {
                c.method == "GET"
                    && c.path == format!("/api/webhooks/{}/{}", theirs.id, theirs.token)
            })
            .map(|c| c.auth.clone())
            .collect::<Vec<_>>();
        assert_eq!(read, [""], "read once, with its token alone");

        let mut aa = link(&f, "aa", &key, &["trade"]);
        assert_eq!(
            say(&mut aa, "r1", "trade", "Ann@aa", "wts pyreal"),
            Outcome::Ok
        );
        let lines = said_in_channel(&d, channel, 1);
        assert_eq!(
            (
                &lines[0]["webhook_id"],
                &lines[0]["author"]["username"],
                &lines[0]["content"]
            ),
            (&json!(theirs.id), &json!("Ann@aa"), &json!("wts pyreal"))
        );
        assert_eq!(d.lock().expect("the stand-in").channel_reads(channel), 0);
        let audit = f.running.registry().audit("aa").expect("the log");
        let shown = [
            op(&f, &session, "GET", route, &Value::Null).body,
            op(&f, &admin, "GET", "/v1/admin/discord", &Value::Null).body,
            audit.to_string(),
            op(&f, &admin, "GET", "/v1/admin/discord/log", &Value::Null).body,
        ];
        for s in shown {
            assert!(!s.contains(&theirs.token), "the token shown: {s}");
            assert!(
                !s.contains("discord.com/api/webhooks"),
                "the address shown: {s}"
            );
        }
        aa.close();
    }

    /// Spec: registry: `discord` is reserved: no world registers it, and a tell to `Name@discord`
    /// is answered as one to a world not on the network.
    #[test]
    fn discord_is_a_slug_no_world_may_take() {
        let (stub, _d) = stand_in();
        let f = hub(&stub);
        let reg = json!({
            "slug": "discord", "name": "Discord", "ruleset": "PvE",
            "endpoint": { "mode": "manual", "address": "192.0.2.1", "port": 9000 },
            "emulator": { "family": "ACE" },
        });
        assert!(
            Registration::from_json(&reg).is_err(),
            "the hub's operator cannot register it"
        );
        let operator = sign_in(&f, "80351110224678930");
        let r = op(&f, &operator, "POST", "/v1/operator/worlds", &reg);
        assert_eq!(r.status, 400, "nor can a world's operator: {r:?}");
        let mut aa = world(&f, "aa", &["global"]);
        aa.send(&Message::Tell {
            id: "t1".into(),
            from: a("Ann@aa"),
            to: a("Nelly@discord"),
            text: "hi".into(),
            friend: false,
        })
        .expect("sent");
        let outcome = aa
            .expect(WAIT, "the tell's answer", |m| match m {
                Message::TellResult { id, outcome, .. } if id == "t1" => Some(*outcome),
                _ => None,
            })
            .expect("an answer");
        assert_eq!(outcome, Outcome::NotOnNetwork);
        aa.close();
    }

    /// Spec: hub: the hub's log says when a webhook an operator gave is gone, and never holds a
    /// line's text, the bot token or a webhook's token or address.
    #[test]
    fn the_log_says_when_a_given_webhook_is_gone_and_never_holds_a_secret() {
        let dir = folder("mirrors-log");
        let (stub, d) = stand_in();
        server_with_channel(&d, &[]);
        let (guild, channel) = ("1558376951000000700", "1558376951000000701");
        server(&d, guild, "80351110224678920", &[]);
        text_channel(&d, guild, channel, json!([]));
        let theirs = make_hook(
            &mut d.lock().expect("the stand-in"),
            channel,
            guild,
            "80351110224678920",
            "Spidey Bot",
        );
        let db = dir.join("hub.sqlite").to_str().expect("a path").to_owned();
        let keys: Vec<String> = {
            let r = Registry::open(&db, "dereth.network", Box::new(std::time::SystemTime::now))
                .expect("a registry");
            let keys = ["aa", "bb"]
                .iter()
                .map(|slug| {
                    let (_, key) = r.register(&super::registration(slug)).expect("registered");
                    r.set_federation(slug, Federation::On).expect("on");
                    key
                })
                .collect();
            r.set_mirror(None, "global", &Placement::bot(CHANNEL, GUILD), None)
                .expect("mirrored");
            let (hook, _) = dereth_network_hub::broker::Rest::new(broker_for(&stub))
                .webhook(&theirs.id, &theirs.token)
                .expect("read");
            r.set_mirror(
                Some("aa"),
                "trade",
                &Placement {
                    channel: channel.into(),
                    server: guild.into(),
                    hook: Some(hook),
                },
                None,
            )
            .expect("mirrored");
            keys
        };
        let token_file = dir.join("discord-bot-token");
        std::fs::write(&token_file, BOT_TOKEN).expect("written");
        let mut child = Command::new(env!("CARGO_BIN_EXE_dereth-network-hub"))
            .args([
                "serve",
                "--listen",
                "127.0.0.1:0",
                "--run-for",
                "120",
                "--db",
                &db,
                "--discord-bot-token-file",
                token_file.to_str().expect("a path"),
                "--discord-api",
                &format!("http://{}/api", stub.addr),
            ])
            .env_remove("DERETH_HUB_DISCORD_BOT_TOKEN")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the hub runs");
        let out = Output::of(&mut child);
        out.says("listening on http://");
        let addr = {
            let log = out.log.lock().expect("the log");
            let at = log
                .split("listening on http://")
                .nth(1)
                .expect("an address");
            at.split_whitespace().next().expect("an address").to_owned()
        };
        hook_in(&d, CHANNEL);
        let mut aa = link_at(&addr, "aa", &keys[0], &["global", "trade"]);
        let mut bb = link_at(&addr, "bb", &keys[1], &["global"]);
        assert_eq!(
            say(&mut aa, "q1", "global", "Ann@aa", "the first secret"),
            Outcome::Ok
        );
        assert_eq!(next_line(&mut bb).2, "the first secret");
        said_in_channel(&d, CHANNEL, 1);
        assert_eq!(
            say(&mut aa, "q2", "trade", "Ann@aa", "the second secret"),
            Outcome::Ok
        );
        said_in_channel(&d, channel, 1);
        d.lock()
            .expect("the stand-in")
            .hooks
            .retain(|h| h.id != theirs.id);
        assert_eq!(
            say(&mut aa, "q3", "trade", "Bob@aa", "the third secret"),
            Outcome::Ok
        );
        out.says("no longer has the webhook given");
        aa.close();
        bb.close();
        let log = out.all(&mut child);
        assert!(log.contains("2 mirrors into Discord"), "{log}");
        for secret in [
            BOT_TOKEN,
            theirs.token.as_str(),
            "the first secret",
            "the second secret",
            "the third secret",
            "api/webhooks",
        ] {
            assert!(!log.contains(secret), "{secret} in the log:\n{log}");
        }
        drop(stub);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- tells into a person's Discord --------------------------------------------------------------

    mod tells {
        //! Tells into a person's Discord on a running hub: a tell to a linked character, its world
        //! saying the character is not online but reachable, said by the bot in the person's direct
        //! messages while that character is away, with no setting turned on, naming the teller and
        //! that character and nothing else of the person; the teller answered as ever; the person's
        //! reply back as a tell from that character, within the reply window only; a refusal by
        //! Discord holding the person's tells back for a while; Discord's waits and the person's
        //! pace holding tells back, counted; and the bot's own lines never read back.
        //! Fixture: the bridge tests' stand-in for Discord, with the bot's direct messages; the
        //! operator tests' hub with sign-in and the broker on, reading every 50 ms; a person signed
        //! in with characters linked by the codes their worlds asked for; worlds linked through the
        //! conformance suite's links, answering tells and the hub's reach questions as each test
        //! says.

        use std::time::Duration;

        use dereth_network_conformance::hub::Link;
        use dereth_network_hub::broker::Broker;
        use dereth_network_protocol::limits::Rate;
        use dereth_network_protocol::{Message, Outcome, PersonOp, PersonOutcome, ReachMessage};
        use serde_json::{json, Value};

        use super::super::super::operators::{hub_with, op, sign_in, Fixture, Stub, ADMIN};
        use super::super::broker_for;
        use super::super::within;
        use super::{a, stand_in, world, Shared, WAIT};

        /// The person's Discord user id.
        const PERSON: &str = "80351110224678930";

        /// Another person's.
        const BOBS: &str = "80351110224678931";

        /// A hub carrying tells into Discord, its worlds `aa`, `bb` and `cc` linked, and the
        /// person signed in, with `characters` linked (each by the code its world asked for).
        struct Net {
            f: Fixture,
            d: Shared,
            _stub: Stub,
            at_aa: Link,
            at_bb: Link,
            at_cc: Link,
        }

        impl Net {
            fn link_of(&mut self, slug: &str) -> &mut Link {
                match slug {
                    "aa" => &mut self.at_aa,
                    "bb" => &mut self.at_bb,
                    _ => &mut self.at_cc,
                }
            }

            /// Waits until the hub has taken everything `slug`'s link sent so far: a request sent
            /// after them on the same link is answered only after they are taken.
            fn synced(&mut self, slug: &str) {
                let l = self.link_of(slug);
                let id = format!("{slug}-synced");
                l.send_raw(
                    &json!({ "type": "channel", "id": id, "op": "list", "channel": "joined",
                             "by": format!("Sync@{slug}") })
                    .to_string(),
                )
                .expect("sent");
                l.expect(WAIT, "the list", |m| {
                    matches!(m, Message::ChannelResult { id: i, .. } if *i == id).then_some(())
                })
                .expect("taken");
            }

            /// What the admin route says of the tells into Discord.
            fn counts(&self) -> Value {
                let admin = sign_in(&self.f, ADMIN);
                let r = op(&self.f, &admin, "GET", "/v1/admin/discord", &Value::Null);
                assert_eq!(r.status, 200, "{r:?}");
                r.json()["tells"].clone()
            }

            /// `who`'s world says it is online on the network, or no longer, and the hub has
            /// taken that.
            fn plays(&mut self, who: &str, on: bool) {
                let who = a(who);
                let slug = who.world().to_owned();
                self.link_of(&slug)
                    .send(&Message::PersonWatch { who, on })
                    .expect("sent");
                self.synced(&slug);
            }

            /// `from` tells `to`, whose world answers `outcome`: the teller's answer.
            fn tell(
                &mut self,
                id: &str,
                from: &str,
                to: &str,
                text: &str,
                outcome: Outcome,
            ) -> Outcome {
                let (teller, target) = (a(from), a(to));
                self.link_of(teller.world())
                    .send(&Message::Tell {
                        id: id.into(),
                        from: teller.clone(),
                        to: target.clone(),
                        text: text.into(),
                        friend: false,
                    })
                    .expect("sent");
                let hub_id = self
                    .link_of(target.world())
                    .expect(WAIT, "the tell", |m| match m {
                        Message::Tell { id, from, .. } if from.same(&teller) => Some(id.clone()),
                        _ => None,
                    })
                    .expect("the tell reaches its world");
                self.link_of(target.world())
                    .send(&Message::TellResult {
                        id: hub_id,
                        outcome,
                        message: None,
                    })
                    .expect("sent");
                let id = id.to_owned();
                self.link_of(teller.world())
                    .expect(WAIT, "the tell's answer", |m| match m {
                        Message::TellResult {
                            id: i,
                            outcome,
                            message: None,
                        } if *i == id => Some(*outcome),
                        _ => None,
                    })
                    .expect("an answer")
            }

            /// `world` answers the hub's next reach question, and the hub has taken the answer:
            /// the teller and the character asked about.
            fn reach(&mut self, world: &str, reachable: bool) -> (String, String) {
                let ReachMessage::Ask { from, to, .. } = self.question(world, reachable) else {
                    unreachable!("a question")
                };
                (from.to_string(), to.to_string())
            }

            /// `world` answers the hub's next reach question, and the hub has taken the answer:
            /// the question.
            fn question(&mut self, world: &str, reachable: bool) -> ReachMessage {
                let l = self.link_of(world);
                let asked = l
                    .expect_reach(WAIT, "a reach question", |m| {
                        matches!(m, ReachMessage::Ask { .. }).then(|| m.clone())
                    })
                    .expect("a question");
                let ReachMessage::Ask { id, .. } = &asked else {
                    unreachable!("a question")
                };
                l.send_raw(
                    &ReachMessage::Result {
                        id: id.clone(),
                        reachable,
                    }
                    .to_json(),
                )
                .expect("sent");
                self.synced(world);
                asked
            }

            /// The person signed in as `session` links `who`, by the code `who`'s world asked
            /// for while it had it on the network.
            fn link(&mut self, session: &str, who: &str) {
                let who = a(who);
                let id = format!("code-{}", who.key());
                let l = self.link_of(who.world());
                super::super::super::persons::go_on(l, &who.to_string());
                l.send(&Message::Person {
                    id: id.clone(),
                    op: PersonOp::Code,
                    who: who.clone(),
                    target: None,
                    targets: Vec::new(),
                    shown: None,
                })
                .expect("sent");
                let code = l
                    .expect(WAIT, "a code", |m| match m {
                        Message::PersonResult {
                            id: i,
                            outcome: PersonOutcome::Ok,
                            code: Some(code),
                            ..
                        } if *i == id => Some(code.clone()),
                        _ => None,
                    })
                    .expect("a code");
                let r = op(
                    &self.f,
                    session,
                    "POST",
                    "/v1/person/link",
                    &json!({ "code": code }),
                );
                assert_eq!(r.status, 200, "{r:?}");
            }

            /// The bot's lines in the person's direct messages, once there are `n`.
            fn dms(&self, n: usize) -> Vec<Value> {
                within("the direct messages", || {
                    let lines = dms_of(&self.d);
                    (lines.len() >= n).then_some(lines)
                })
            }

            /// The person writes `content` in its direct messages, replying to the line
            /// `reply_to` when given: the line's id.
            fn person_says(&self, content: &str, reply_to: Option<&str>) -> String {
                let mut d = self.d.lock().expect("the stand-in");
                let id = d.id();
                let channel = d
                    .dms
                    .iter()
                    .find(|(_, m)| m.recipient == PERSON)
                    .map(|(c, _)| c.clone())
                    .expect("the direct messages are open");
                let mut line = json!({
                    "id": id, "type": 0, "channel_id": channel, "content": content,
                    "author": { "id": PERSON, "username": "nelly", "global_name": "Nelly" },
                    "mentions": [], "attachments": [],
                });
                if let Some(to) = reply_to {
                    line["type"] = json!(19);
                    line["message_reference"] = json!({ "message_id": to, "channel_id": channel });
                }
                d.dms
                    .get_mut(&channel)
                    .expect("the channel")
                    .lines
                    .push(line);
                id
            }

            /// How many times the person's direct messages were read.
            fn reads(&self) -> usize {
                let d = self.d.lock().expect("the stand-in");
                d.dms
                    .iter()
                    .find(|(_, m)| m.recipient == PERSON)
                    .map_or(0, |(c, _)| {
                        d.count("GET", &format!("/api/channels/{c}/messages?"))
                    })
            }
        }

        /// The bot's lines in the person's direct messages, oldest first.
        fn dms_of(d: &Shared) -> Vec<Value> {
            d.lock()
                .expect("the stand-in")
                .dms
                .values()
                .filter(|m| m.recipient == PERSON)
                .flat_map(|m| m.lines.iter())
                .filter(|l| l["author"]["bot"] == true)
                .cloned()
                .collect()
        }

        /// The net, its broker as `tune` leaves it.
        fn net(characters: &[&str], tune: impl FnOnce(&mut Broker)) -> Net {
            let (stub, d) = stand_in();
            d.lock()
                .expect("the stand-in")
                .reachable
                .insert(PERSON.into());
            let mut broker = broker_for(&stub);
            tune(&mut broker);
            let f = hub_with(move |c| c.broker = Some(broker));
            let (at_aa, at_bb, at_cc) = (
                world(&f, "aa", &[]),
                world(&f, "bb", &[]),
                world(&f, "cc", &[]),
            );
            let session = sign_in(&f, PERSON);
            let mut net = Net {
                f,
                d,
                _stub: stub,
                at_aa,
                at_bb,
                at_cc,
            };
            for who in characters {
                net.link(&session, who);
            }
            net
        }

        /// Spec: hub: a tell to a linked character, which its world says is not online but
        /// reachable, is said by the bot in the person's direct messages with no setting turned
        /// on, naming the teller and the character, calling nobody and previewing no link, while
        /// that character is away, whatever the person's other characters do; the teller is
        /// answered `offline`, as for any character not online.
        #[test]
        fn a_linked_characters_tell_is_said_on_discord_while_it_is_away() {
            let mut n = net(&["Ann@bb", "Ann@cc"], |_| {});
            assert_eq!(
                n.tell("t1", "Bob@aa", "Ann@bb", "still playing?", Outcome::Offline),
                Outcome::Offline,
                "the teller's answer is the world's"
            );
            assert_eq!(
                n.reach("bb", true),
                ("Bob@aa".into(), "Ann@bb".into()),
                "the character's world is asked"
            );
            let dm = &n.dms(1)[0];
            assert_eq!(
                dm["content"],
                "Bob@aa tells Ann@bb, \"still playing?\"\n-# Reply to answer as Ann@bb."
            );
            assert_eq!(dm["allowed_mentions"], json!({ "parse": [] }));
            assert_eq!(dm["flags"], 4, "previews no link");
            {
                let d = n.d.lock().expect("the stand-in");
                let open = d
                    .calls
                    .iter()
                    .find(|c| c.path == "/api/users/@me/channels")
                    .expect("the direct messages opened");
                assert_eq!(open.auth, "Bot");
                assert_eq!(open.body, json!({ "recipient_id": PERSON }));
            }

            // Another of the person's characters plays: the next tell is said there all the same.
            // The character told plays: the one after is not, until it plays no more.
            n.plays("Ann@cc", true);
            n.tell("t2", "Bob@aa", "Ann@bb", "other one on?", Outcome::Offline);
            n.reach("bb", true);
            n.dms(2);
            n.plays("Ann@bb", true);
            n.tell("t3", "Bob@aa", "Ann@bb", "online now?", Outcome::Offline);
            n.reach("bb", true);
            n.plays("Ann@bb", false);
            n.tell("t4", "Bob@aa", "Ann@bb", "gone again?", Outcome::Offline);
            n.reach("bb", true);
            let dms = n.dms(3);
            assert!(
                dms[2]["content"]
                    .as_str()
                    .is_some_and(|c| c.contains("gone again?")),
                "nothing went while the character played: {dms:?}"
            );
            assert_eq!(dms.len(), 3);
        }

        /// Spec: hub: the direct message names the teller and the character told and none of the
        /// person's others; the teller learns nothing it would not of a character nobody linked; no
        /// other world of the person's hears anything; and the person answers as the character the
        /// tell was for.
        #[test]
        fn the_direct_message_names_the_character_told_alone_and_the_reply_answers_as_it() {
            let mut n = net(&["Ann Two@cc", "Ann@bb"], |_| {});
            n.plays("Ann Two@cc", true);
            assert_eq!(
                n.tell("t1", "Bob@aa", "Ann@bb", "who are you?", Outcome::Offline),
                Outcome::Offline
            );
            n.reach("bb", true);
            let dm = n.dms(1)[0]["content"].as_str().unwrap_or("").to_owned();
            assert!(dm.contains("Ann@bb") && !dm.contains("Ann Two"), "{dm}");
            n.person_says("just me", None);
            let from = n
                .at_aa
                .expect(WAIT, "the reply", |m| match m {
                    Message::Tell { from, to, text, .. } if to.same(&a("Bob@aa")) => {
                        Some((from.to_string(), text.clone()))
                    }
                    _ => None,
                })
                .expect("the reply");
            assert_eq!(from, ("Ann@bb".into(), "just me".into()));
            n.at_cc
                .quiet(Duration::from_millis(500), "anything of the tell", |m| {
                    matches!(m, Message::Tell { .. } | Message::TellResult { .. })
                })
                .expect("the person's other world hears nothing");
            assert!(n.at_cc.take_reach().is_empty());
        }

        /// Spec: hub: whether a tell goes to Discord is the character's world's to say, by its
        /// squelches and blocks; the reply's is the teller's world's, as for any tell,
        /// and the person is told what the character would be (not available), and nothing for a
        /// reply its recipient squelches.
        #[test]
        fn the_characters_world_decides_who_reaches_it_and_the_tellers_world_its_reply() {
            let mut n = net(&["Ann@bb"], |_| {});
            n.tell("t1", "Bob@aa", "Ann@bb", "squelched me?", Outcome::Offline);
            n.reach("bb", false);
            n.tell("t2", "Bob@aa", "Ann@bb", "reachable", Outcome::Offline);
            n.reach("bb", true);
            let dms = n.dms(1);
            assert!(
                dms[0]["content"]
                    .as_str()
                    .is_some_and(|c| c.contains("reachable")),
                "the world said no to the first: {dms:?}"
            );

            n.person_says("first answer", None);
            let answer = |n: &mut Net, outcome: Outcome| {
                let id = n
                    .at_aa
                    .expect(WAIT, "the reply", |m| match m {
                        Message::Tell { id, from, .. } if from.same(&a("Ann@bb")) => {
                            Some(id.clone())
                        }
                        _ => None,
                    })
                    .expect("the reply");
                n.at_aa
                    .send(&Message::TellResult {
                        id,
                        outcome,
                        message: None,
                    })
                    .expect("sent");
            };
            answer(&mut n, Outcome::Squelched);
            n.person_says("second answer", None);
            answer(&mut n, Outcome::Offline);
            let dms = n.dms(2);
            assert_eq!(dms[1]["content"], "Bob@aa is not available.");
            assert_eq!(dms.len(), 2, "nothing for the squelched one");
        }

        /// Spec: hub: the person's line in its direct messages answers, as the character the tell
        /// was for, the tell it replies to (Discord's Reply), or else the latest, as a tell from
        /// that character to the teller's world.
        #[test]
        fn a_reply_comes_back_as_a_tell_from_the_character_to_the_teller_it_answers() {
            let mut n = net(&["Ann@bb"], |_| {});
            n.tell("t1", "Bob@aa", "Ann@bb", "from Bob", Outcome::Offline);
            n.reach("bb", true);
            n.tell("t2", "Cat@cc", "Ann@bb", "from Cat", Outcome::Offline);
            n.reach("bb", true);
            let dms = n.dms(2);
            let to_bob = dms[0]["id"].as_str().expect("an id").to_owned();
            n.person_says("hello **Bob**", Some(&to_bob));
            let got = n
                .at_aa
                .expect(WAIT, "the reply", |m| match m {
                    Message::Tell { from, to, text, .. } => {
                        Some((from.to_string(), to.to_string(), text.clone()))
                    }
                    _ => None,
                })
                .expect("the reply");
            assert_eq!(
                got,
                ("Ann@bb".into(), "Bob@aa".into(), "hello **Bob**".into())
            );
            n.person_says("and you, Cat", None);
            let got = n
                .at_cc
                .expect(WAIT, "the reply", |m| match m {
                    Message::Tell { from, to, text, .. } => {
                        Some((from.to_string(), to.to_string(), text.clone()))
                    }
                    _ => None,
                })
                .expect("the reply");
            assert_eq!(
                got,
                ("Ann@bb".into(), "Cat@cc".into(), "and you, Cat".into()),
                "the latest"
            );
            assert_eq!(n.counts()["fromDiscord"], 2);
        }

        /// Spec: hub: a linked character its world takes off the network pauses its link: a tell
        /// to it goes nowhere else (its world is not even asked), and the person's reply to an
        /// earlier tell is not carried, counted as not carried; back on the network, with nothing
        /// to redo, its tells reach the person's Discord again.
        #[test]
        fn a_paused_link_carries_no_tell_into_discord_and_no_reply_back() {
            use super::super::super::persons::{go_off, go_on};
            let mut n = net(&["Ann@bb"], |_| {});
            n.tell("t1", "Bob@aa", "Ann@bb", "before", Outcome::Offline);
            n.reach("bb", true);
            n.dms(1);
            go_off(&mut n.at_bb, "Ann@bb");
            assert_eq!(
                n.tell("t2", "Bob@aa", "Ann@bb", "while off", Outcome::Offline),
                Outcome::Offline
            );
            assert!(
                n.at_bb
                    .expect_reach(Duration::from_secs(1), "a reach question", |m| Some(
                        m.clone()
                    ))
                    .is_err(),
                "its world is not asked"
            );
            n.person_says("a reply while off", None);
            within("the reply not carried", || {
                (n.counts()["notCarried"] == 1).then_some(())
            });
            n.at_aa
                .quiet(Duration::from_millis(300), "a reply", |m| {
                    matches!(m, Message::Tell { .. })
                })
                .expect("nothing reaches the teller");
            go_on(&mut n.at_bb, "Ann@bb");
            n.tell("t3", "Bob@aa", "Ann@bb", "back on", Outcome::Offline);
            n.reach("bb", true);
            let dms = n.dms(2);
            assert!(
                dms[1]["content"]
                    .as_str()
                    .is_some_and(|c| c.contains("back on")),
                "{dms:?}"
            );
            assert_eq!(dms.len(), 2, "nothing went while it was off");
        }

        /// Spec: hub: past the reply window nothing of the person's direct messages is read, and
        /// no line said there after it reaches any world.
        #[test]
        fn a_reply_past_the_window_is_never_read() {
            let mut n = net(&["Ann@bb"], |b| b.reply_window = Duration::from_millis(300));
            n.tell("t1", "Bob@aa", "Ann@bb", "quick", Outcome::Offline);
            n.reach("bb", true);
            n.dms(1);
            std::thread::sleep(Duration::from_millis(600));
            let reads = n.reads();
            n.person_says("too late", None);
            n.at_aa
                .quiet(Duration::from_secs(1), "a reply", |m| {
                    matches!(m, Message::Tell { .. })
                })
                .expect("nothing past the window");
            assert!(
                n.reads() <= reads + 1,
                "the person's direct messages are read no more"
            );
        }

        /// Spec: hub: a direct message Discord refuses holds the person's tells back for a while:
        /// none is tried meanwhile, each counted as not carried; once it is over, they are tried
        /// again.
        #[test]
        fn a_refused_dm_holds_the_persons_tells_back_for_a_while() {
            let mut n = net(&["Ann@bb"], |b| {
                b.refused_for = Duration::from_millis(1_500)
            });
            n.d.lock().expect("the stand-in").reachable.remove(PERSON);
            n.tell(
                "t1",
                "Bob@aa",
                "Ann@bb",
                "can you hear me?",
                Outcome::Offline,
            );
            n.reach("bb", true);
            within("the first not carried", || {
                (n.counts()["notCarried"] == 1).then_some(())
            });
            let tried =
                n.d.lock()
                    .expect("the stand-in")
                    .count("POST", "/api/channels/");
            n.tell("t2", "Bob@aa", "Ann@bb", "still?", Outcome::Offline);
            n.reach("bb", true);
            within("the second not carried", || {
                (n.counts()["notCarried"] == 2).then_some(())
            });
            assert_eq!(
                n.d.lock()
                    .expect("the stand-in")
                    .count("POST", "/api/channels/"),
                tried,
                "Discord is not asked again"
            );
            n.d.lock()
                .expect("the stand-in")
                .reachable
                .insert(PERSON.into());
            std::thread::sleep(Duration::from_millis(1_600));
            n.tell("t3", "Bob@aa", "Ann@bb", "now?", Outcome::Offline);
            n.reach("bb", true);
            assert!(n.dms(1)[0]["content"]
                .as_str()
                .is_some_and(|c| c.contains("now?")));
        }

        /// Spec: hub: while Discord asks the hub to wait on the direct messages' route, tells into
        /// them are held back, counted, without asking Discord anything.
        #[test]
        fn discords_wait_holds_tells_back_without_asking_it() {
            let mut n = net(&["Ann@bb"], |_| {});
            n.d.lock()
                .expect("the stand-in")
                .busy
                .insert("/users/@me/channels".into(), (60, 1));
            for (id, text) in [("t1", "one"), ("t2", "two")] {
                n.tell(id, "Bob@aa", "Ann@bb", text, Outcome::Offline);
                n.reach("bb", true);
            }
            within("both held back", || {
                (n.counts()["heldBackToDiscord"] == 2).then_some(())
            });
            let d = n.d.lock().expect("the stand-in");
            assert_eq!(
                d.count("POST", "/api/users/@me/channels"),
                1,
                "asked once, and not within the wait"
            );
            assert!(d.dms.is_empty(), "never opened");
        }

        /// Spec: hub: tells into one person's Discord past its pace are held back and counted.
        #[test]
        fn tells_past_a_persons_pace_are_held_back_and_counted() {
            let mut n = net(&["Ann@bb"], |b| {
                b.dm_pace = Rate {
                    burst: 2.0,
                    per_sec: 1.0 / 86_400.0,
                };
            });
            for (id, text) in [("t1", "one"), ("t2", "two"), ("t3", "three")] {
                n.tell(id, "Bob@aa", "Ann@bb", text, Outcome::Offline);
                n.reach("bb", true);
            }
            let counts = within("all three handled", || {
                let c = n.counts();
                (c["toDiscord"] == 2 && c["heldBackToDiscord"] == 1).then_some(c)
            });
            assert_eq!(counts["notCarried"], 0);
            assert_eq!(n.dms(2).len(), 2);
        }

        /// Spec: hub: the bot's own lines in a person's direct messages, its tells and its notes,
        /// are never read back as the person's, and each of the person's lines is carried once.
        #[test]
        fn the_bots_own_lines_never_come_back_and_each_reply_goes_once() {
            let mut n = net(&["Ann@bb"], |_| {});
            n.tell("t1", "Bob@aa", "Ann@bb", "ping", Outcome::Offline);
            n.reach("bb", true);
            n.dms(1);
            n.person_says("pong", None);
            let reply = |n: &mut Net| {
                n.at_aa
                    .expect(WAIT, "a reply", |m| match m {
                        Message::Tell { id, text, .. } => Some((id.clone(), text.clone())),
                        _ => None,
                    })
                    .expect("a reply")
            };
            let (id, text) = reply(&mut n);
            assert_eq!(text, "pong");
            n.at_aa
                .send(&Message::TellResult {
                    id,
                    outcome: Outcome::Offline,
                    message: None,
                })
                .expect("sent");
            n.dms(2);
            let reads = n.reads();
            within("two more reads", || (n.reads() >= reads + 2).then_some(()));
            n.person_says("pong again", None);
            assert_eq!(
                reply(&mut n).1,
                "pong again",
                "nothing of the bot's, and nothing twice, came first"
            );
        }

        /// Spec: hub: two persons, both away, answer each other in their direct messages: a
        /// reply to a teller not online goes on to the teller's person's direct messages when the
        /// teller's world says the replying character may reach it, as any tell does, and the
        /// replying person is told the teller is not available, as any teller is.
        #[test]
        fn two_persons_away_answer_each_other_through_their_direct_messages() {
            let mut n = net(&["Ann@bb"], |_| {});
            n.d.lock()
                .expect("the stand-in")
                .reachable
                .insert(BOBS.into());
            let bobs = sign_in(&n.f, BOBS);
            n.link(&bobs, "Bob@aa");
            let lines_of = |n: &Net, user: &str| -> Vec<String> {
                n.d.lock()
                    .expect("the stand-in")
                    .dms
                    .values()
                    .filter(|m| m.recipient == user)
                    .flat_map(|m| m.lines.iter())
                    .filter(|l| l["author"]["bot"] == true)
                    .map(|l| l["content"].as_str().unwrap_or("").to_owned())
                    .collect()
            };
            let said = |n: &Net, user: &str, count: usize| {
                within("the direct messages", || {
                    let lines = lines_of(n, user);
                    (lines.len() >= count).then_some(lines)
                })
            };
            // A reply reaches the teller's world, which says the teller is not online.
            let away = |n: &mut Net, world: &str, from: &str| {
                let l = n.link_of(world);
                let id = l
                    .expect(WAIT, "the reply", |m| match m {
                        Message::Tell { id, from: f, .. } if f.same(&a(from)) => Some(id.clone()),
                        _ => None,
                    })
                    .expect("the reply");
                l.send(&Message::TellResult {
                    id,
                    outcome: Outcome::Offline,
                    message: None,
                })
                .expect("sent");
            };

            n.tell("t1", "Bob@aa", "Ann@bb", "hello Ann", Outcome::Offline);
            n.reach("bb", true);
            assert!(said(&n, PERSON, 1)[0].contains("hello Ann"));
            n.person_says("hi Bob, I am away too", None);
            away(&mut n, "aa", "Ann@bb");
            assert_eq!(
                n.reach("aa", true),
                ("Ann@bb".into(), "Bob@aa".into()),
                "the teller's world is asked whether the replying character may reach it"
            );
            let to_bob = said(&n, BOBS, 1);
            assert!(
                to_bob[0].contains("Ann@bb") && to_bob[0].contains("hi Bob, I am away too"),
                "{to_bob:?}"
            );
            assert_eq!(
                said(&n, PERSON, 2)[1],
                "Bob@aa is not available.",
                "told as any teller is"
            );

            // Bob answers from his direct messages, and it reaches Ann's the same way.
            {
                let mut d = n.d.lock().expect("the stand-in");
                let id = d.id();
                let channel = d
                    .dms
                    .iter()
                    .find(|(_, m)| m.recipient == BOBS)
                    .map(|(c, _)| c.clone())
                    .expect("Bob's direct messages are open");
                let line = json!({
                    "id": id, "type": 0, "channel_id": channel, "content": "talk later then",
                    "author": { "id": BOBS, "username": "bob", "global_name": "Bob" },
                    "mentions": [], "attachments": [],
                });
                d.dms
                    .get_mut(&channel)
                    .expect("the channel")
                    .lines
                    .push(line);
            }
            away(&mut n, "bb", "Bob@aa");
            assert_eq!(n.reach("bb", true), ("Bob@aa".into(), "Ann@bb".into()));
            let to_ann = said(&n, PERSON, 3);
            assert!(
                to_ann[2].contains("Bob@aa") && to_ann[2].contains("talk later then"),
                "{to_ann:?}"
            );
            assert_eq!(n.counts()["fromDiscord"], 2);
        }
    }

    // ---- speaking from Discord ----------------------------------------------------------------------

    pub(super) mod speak {
        //! Speaking in a mirror from Discord on a running hub: Discord's interactions taken only
        //! signed with the application's key, once, and the `/dn` command registered with the bot;
        //! `/dn identity`'s menu of the person's own characters on the channel's world, its choice
        //! kept and only ever one of them; `/dn say` asking the world to say the line as the
        //! person's one character there or the one it chose, and with several and none chosen,
        //! answering with the same menu, whose choice says the line and is kept; the world's line
        //! going on marked to the other worlds and to every mirror of the room, its answer, or its
        //! refusal, edited into the person's answer; nothing said from an admins' mirror or a
        //! channel that mirrors nothing; the person's pace; and every answer the person's alone.
        //! Fixture: the bridge tests' stand-in for Discord, which gives the application's public key
        //! (a key pair the tests sign with), takes the command and the edits; the operator tests'
        //! hub with sign-in and the broker on; a server whose channels mirror the world `aa`'s
        //! Global room (its operator's mirror) and the network's (the admins'), and one that mirrors
        //! nothing; worlds linked through the conformance suite's links, the test answering the
        //! hub's `say` for the world as each test says.

        use std::time::{Duration, SystemTime};

        use dereth_network_conformance::hub::Link;
        use dereth_network_protocol::say::SayOutcome;
        use dereth_network_protocol::{Message, PersonOp, PersonOutcome, SayMessage};
        use ring::signature::{Ed25519KeyPair, KeyPair as _};
        use serde_json::{json, Value};

        use super::super::super::operators::{
            call, hub_with, op, sign_in, Answer, Fixture, Stub, ADMIN,
        };
        use super::super::{broker_for, within};
        use super::{
            a, admin_mirrors, hook_in, link, register, server, stand_in, text_channel, Shared, APP,
            BOT, GUILD, GUILD_OWNER, HOOKS_ROLE, WAIT,
        };

        /// The application's key pair's seed: the stand-in gives its public key, the tests sign
        /// with it.
        const SEED: [u8; 32] = [7; 32];

        /// The person's Discord user id; another person's; and a Discord user who is no person.
        const PERSON: &str = "80351110224678940";
        const OTHER: &str = "80351110224678941";
        const STRANGER: &str = "80351110224678942";
        /// The world `aa`'s operator.
        const OWNER: &str = "80351110224678943";

        /// The channel that mirrors `aa`'s Global room, the admins' mirror of it, and a channel
        /// that mirrors nothing.
        const AAS: &str = "1558376951000000301";
        const ADMINS: &str = "1558376951000000302";
        const PLAIN: &str = "1558376951000000303";

        fn pair() -> Ed25519KeyPair {
            Ed25519KeyPair::from_seed_unchecked(&SEED).expect("a key pair")
        }

        fn hex(b: &[u8]) -> String {
            b.iter().map(|x| format!("{x:02x}")).collect()
        }

        /// The application's public key, as Discord gives it.
        pub(in super::super) fn public_key() -> String {
            hex(pair().public_key().as_ref())
        }

        /// The hub, its worlds `aa` and `bb` linked carrying the Global room, `aa`'s operator
        /// mirroring it into [`AAS`] and the admins into [`ADMINS`]; the person signed in, linking
        /// `Ann@aa` and `Bo@aa`, and the other person `Cy@aa`.
        struct Net {
            f: Fixture,
            d: Shared,
            _stub: Stub,
            at_aa: Link,
            at_bb: Link,
            key_aa: String,
            next: u64,
        }

        impl Net {
            fn new() -> Self {
                Self::with_allowance(100_000)
            }

            /// [`Net::new`], each client address allowed `per_minute` requests a minute.
            fn with_allowance(per_minute: u32) -> Self {
                let (stub, d) = stand_in();
                server(
                    &d,
                    GUILD,
                    GUILD_OWNER,
                    &[
                        (BOT, &[HOOKS_ROLE], None),
                        (ADMIN, &[HOOKS_ROLE], None),
                        (OWNER, &[HOOKS_ROLE], None),
                        (PERSON, &[], None),
                        (STRANGER, &[], None),
                    ],
                );
                for c in [AAS, ADMINS, PLAIN] {
                    text_channel(&d, GUILD, c, json!([]));
                }
                let broker = broker_for(&stub);
                let f = hub_with(move |c| {
                    c.broker = Some(broker);
                    c.http_per_minute = per_minute;
                });
                let admin = sign_in(&f, ADMIN);
                assert_eq!(
                    admin_mirrors(&f, &admin, "global", &json!({ "channel": ADMINS })).status,
                    200
                );
                let (key_aa, key_bb) = (register(&f, "aa"), register(&f, "bb"));
                let owner = sign_in(&f, OWNER);
                f.running
                    .registry()
                    .assign("aa", OWNER)
                    .expect("handed to its operator");
                let r = op(
                    &f,
                    &owner,
                    "PUT",
                    "/v1/operator/worlds/aa/discord/mirrors/global",
                    &json!({ "channel": AAS }),
                );
                assert_eq!(r.status, 200, "{r:?}");
                let at_aa = link(&f, "aa", &key_aa, &["global"]);
                let at_bb = link(&f, "bb", &key_bb, &["global"]);
                let mut net = Self {
                    f,
                    d,
                    _stub: stub,
                    at_aa,
                    at_bb,
                    key_aa,
                    next: 1_558_376_960_000_000_000,
                };
                let (person, other) = (sign_in(&net.f, PERSON), sign_in(&net.f, OTHER));
                net.link(&person, "Ann@aa");
                net.link(&person, "Bo@aa");
                net.link(&other, "Cy@aa");
                within("the command registered", || {
                    net.d.lock().expect("the stand-in").commands.clone()
                });
                hook_in(&net.d, AAS);
                hook_in(&net.d, ADMINS);
                net
            }

            /// The person signed in as `session` links `who`, by the code `aa` asked for while
            /// it had it on the network.
            fn link(&mut self, session: &str, who: &str) {
                let id = format!("code-{who}");
                super::super::super::persons::go_on(&mut self.at_aa, who);
                self.at_aa
                    .send(&Message::Person {
                        id: id.clone(),
                        op: PersonOp::Code,
                        who: a(who),
                        target: None,
                        targets: Vec::new(),
                        shown: None,
                    })
                    .expect("sent");
                let code = self
                    .at_aa
                    .expect(WAIT, "a code", |m| match m {
                        Message::PersonResult {
                            id: i,
                            outcome: PersonOutcome::Ok,
                            code: Some(code),
                            ..
                        } if *i == id => Some(code.clone()),
                        _ => None,
                    })
                    .expect("a code");
                let r = op(
                    &self.f,
                    session,
                    "POST",
                    "/v1/person/link",
                    &json!({ "code": code }),
                );
                assert_eq!(r.status, 200, "{r:?}");
            }

            /// A fresh interaction id.
            fn id(&mut self) -> String {
                self.next += 1;
                self.next.to_string()
            }

            /// `body` sent to the hub's interactions route, signed with `timestamp` by `key`.
            fn post_as(&self, body: &Value, timestamp: &str, key: &Ed25519KeyPair) -> Answer {
                let body = body.to_string();
                let sig = hex(key
                    .sign(&[timestamp.as_bytes(), body.as_bytes()].concat())
                    .as_ref());
                call(
                    self.f.addr(),
                    "POST",
                    "/v1/discord/interactions",
                    &[
                        ("Content-Type", "application/json"),
                        ("X-Signature-Ed25519", &sig),
                        ("X-Signature-Timestamp", timestamp),
                    ],
                    &body,
                )
            }

            /// `body` sent signed as Discord signs it, now.
            fn post(&self, body: &Value) -> Answer {
                self.post_as(body, &now(), &pair())
            }

            /// `user` uses `/dn <sub>` in `channel`, with `options`: the hub's answer.
            fn command(&mut self, user: &str, channel: &str, sub: &str, options: Value) -> Value {
                let id = self.id();
                let r = self.post(&interaction(
                    &id,
                    2,
                    user,
                    channel,
                    json!({ "name": "dn", "type": 1,
                            "options": [{ "type": 1, "name": sub, "options": options }] }),
                ));
                assert_eq!(r.status, 200, "{r:?}");
                let v = r.json();
                assert_eq!(v["data"]["flags"], 64, "the person's alone: {v}");
                v
            }

            /// `user` uses `/dn identity` in `channel`.
            fn identity(&mut self, user: &str, channel: &str) -> Value {
                self.command(user, channel, "identity", json!([]))
            }

            /// `user` uses `/dn say <text>` in `channel`: the hub's answer, and the interaction's
            /// token, which its edits name.
            fn say(&mut self, user: &str, channel: &str, text: &str) -> (Value, String) {
                let v = self.command(
                    user,
                    channel,
                    "say",
                    json!([{ "type": 3, "name": "text", "value": text }]),
                );
                (v, format!("tok-{}", self.next))
            }

            /// `user` chooses `value` in `/dn identity`'s menu in `channel`: what the menu says
            /// then.
            fn choose(&mut self, user: &str, channel: &str, value: &str) -> String {
                self.choose_in(user, channel, "dn-identity", value).0
            }

            /// `user` chooses `value` in the menu `custom_id` names in `channel`: what the menu
            /// says then, and the interaction's token, which its edits name.
            fn choose_in(
                &mut self,
                user: &str,
                channel: &str,
                custom_id: &str,
                value: &str,
            ) -> (String, String) {
                let id = self.id();
                let r = self.post(&interaction(
                    &id,
                    3,
                    user,
                    channel,
                    json!({ "custom_id": custom_id, "component_type": 3, "values": [value] }),
                ));
                assert_eq!(r.status, 200, "{r:?}");
                let v = r.json();
                assert_eq!(v["type"], 7, "{v}");
                assert_eq!(v["data"]["components"], json!([]), "{v}");
                (
                    v["data"]["content"].as_str().unwrap_or("").to_owned(),
                    format!("tok-{id}"),
                )
            }

            /// The world `aa` answers the hub's next `say` `outcome`, sending the line on first when
            /// it says it, as a world does: the say.
            fn world_answers(&mut self, outcome: SayOutcome, reason: Option<&str>) -> SayMessage {
                let asked = self
                    .at_aa
                    .expect_say(WAIT, "a say", |m| {
                        matches!(m, SayMessage::Ask { .. }).then(|| m.clone())
                    })
                    .expect("a say");
                let SayMessage::Ask {
                    id,
                    room,
                    who,
                    text,
                    via,
                } = &asked
                else {
                    unreachable!("a say")
                };
                if outcome == SayOutcome::Said {
                    self.at_aa
                        .send(&Message::RoomSay {
                            id: format!("r-{id}"),
                            room: room.clone(),
                            from: who.clone(),
                            text: text.clone(),
                            via: Some(via.clone()),
                        })
                        .expect("sent");
                }
                self.at_aa
                    .send_raw(
                        &SayMessage::Result {
                            id: id.clone(),
                            outcome,
                            reason: reason.map(str::to_owned),
                        }
                        .to_json(),
                    )
                    .expect("sent");
                asked
            }

            /// What the answer of the interaction whose token is `token` was edited to say, once it
            /// was.
            fn edited(&self, token: &str) -> String {
                within("the answer edited", || {
                    self.d
                        .lock()
                        .expect("the stand-in")
                        .edits
                        .iter()
                        .find(|(t, _)| t == token)
                        .map(|(_, c)| c.clone())
                })
            }

            /// The lines said through webhooks in `channel` now: each its name and content.
            fn lines_in(&self, channel: &str) -> Vec<(String, String)> {
                self.d
                    .lock()
                    .expect("the stand-in")
                    .channel_lines(channel)
                    .iter()
                    .filter(|l| !l["webhook_id"].is_null())
                    .map(|l| {
                        (
                            l["author"]["username"].as_str().unwrap_or("").to_owned(),
                            l["content"].as_str().unwrap_or("").to_owned(),
                        )
                    })
                    .collect()
            }
        }

        /// The time now, as Discord writes a signature's.
        fn now() -> String {
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs())
                .to_string()
        }

        /// An interaction of `kind` from `user` in `channel` of the stand-in's server.
        fn interaction(id: &str, kind: u64, user: &str, channel: &str, data: Value) -> Value {
            json!({
                "id": id, "application_id": APP, "type": kind, "token": format!("tok-{id}"),
                "version": 1, "guild_id": GUILD, "channel_id": channel,
                "member": { "user": { "id": user, "username": "someone" }, "roles": [] },
                "data": data,
            })
        }

        /// Spec: hub: Discord's interactions are taken only signed by the application's key over
        /// their timestamp and body, within five minutes of the hub's clock, and each once; `PING`
        /// is answered; the command is registered with the bot, in servers alone; the hub says it
        /// lets persons speak.
        #[test]
        fn discord_reaches_the_hub_only_signed_and_only_once() {
            let mut n = Net::new();
            let ping =
                |id: &str| json!({ "id": id, "application_id": APP, "type": 1, "token": "t" });
            let r = n.post(&ping("1"));
            assert_eq!((r.status, r.json()), (200, json!({ "type": 1 })));
            let stranger = Ed25519KeyPair::from_seed_unchecked(&[9; 32]).expect("a key pair");
            assert_eq!(
                n.post_as(&ping("2"), &now(), &stranger).status,
                401,
                "another key"
            );
            let stale = (SystemTime::now() - Duration::from_secs(400))
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs())
                .to_string();
            assert_eq!(
                n.post_as(&ping("3"), &stale, &pair()).status,
                401,
                "too old"
            );
            let unsigned = call(
                n.f.addr(),
                "POST",
                "/v1/discord/interactions",
                &[("Content-Type", "application/json")],
                &ping("4").to_string(),
            );
            assert_eq!(unsigned.status, 401);
            let body = ping("5").to_string();
            let sig = hex(pair()
                .sign(&[now().as_bytes(), body.as_bytes()].concat())
                .as_ref());
            let tampered = call(
                n.f.addr(),
                "POST",
                "/v1/discord/interactions",
                &[
                    ("X-Signature-Ed25519", &sig),
                    ("X-Signature-Timestamp", &now()),
                ],
                &body.replace("\"t\"", "\"u\""),
            );
            assert_eq!(tampered.status, 401, "another body");
            let id = n.id();
            let identity = interaction(
                &id,
                2,
                PERSON,
                AAS,
                json!({ "name": "dn", "options": [{ "type": 1, "name": "identity" }] }),
            );
            assert_eq!(n.post(&identity).status, 200);
            assert_eq!(n.post(&identity).status, 401, "taken once");
            let commands = n.d.lock().expect("the stand-in").commands.clone();
            let commands = commands.expect("registered");
            assert_eq!(commands[0]["name"], "dn");
            assert_eq!(commands[0]["contexts"], json!([0]));
            let calls = n.d.lock().expect("the stand-in").calls.clone();
            assert!(calls
                .iter()
                .filter(|c| c.path.starts_with("/api/applications/"))
                .all(|c| c.auth == "Bot"));
            let config = call(n.f.addr(), "GET", "/v1/config", &[], "").json();
            assert_eq!(config["broker"]["speak"], true);
        }

        /// Spec: hub: `/dn identity` shows the person alone its own characters on the channel's
        /// world in a menu; its choice is kept, and is only ever one of them; a Discord user who
        /// links none, a channel that mirrors nothing and an admins' mirror are answered so.
        #[test]
        fn a_person_chooses_one_of_its_own_characters_on_the_channels_world() {
            let mut n = Net::new();
            let menu = n.identity(PERSON, AAS);
            let options: Vec<&str> = menu["data"]["components"][0]["components"][0]["options"]
                .as_array()
                .expect("a menu")
                .iter()
                .filter_map(|o| o["value"].as_str())
                .collect();
            assert_eq!(options, ["Ann@aa", "Bo@aa"], "its own");
            assert_eq!(
                menu["data"]["content"],
                "Choose which of your characters on World aa you speak as."
            );
            assert_eq!(
                n.choose(PERSON, AAS, "Bo@aa"),
                "You speak as Bo@aa on World aa now."
            );
            let menu = n.identity(PERSON, AAS);
            assert!(menu["data"]["content"]
                .as_str()
                .is_some_and(|c| c.contains("You speak as Bo@aa now")));
            assert_eq!(
                n.choose(PERSON, AAS, "Cy@aa"),
                "That is not one of your characters on World aa.",
                "another person's"
            );
            assert_eq!(
                n.choose(PERSON, AAS, "Ann@bb"),
                "That is not one of your characters on World aa.",
                "another world's"
            );
            let menu = n.identity(PERSON, AAS);
            assert!(
                menu["data"]["content"]
                    .as_str()
                    .is_some_and(|c| c.contains("You speak as Bo@aa now")),
                "nothing refused was kept"
            );
            let stranger = n.identity(STRANGER, AAS);
            assert!(
                stranger["data"]["content"]
                    .as_str()
                    .is_some_and(|c| c.starts_with(
                        "You have no character linked on World aa. To link one, type @dn link in game"
                    )),
                "{stranger}"
            );
            assert_eq!(stranger["data"].get("components"), None);
            assert_eq!(
                n.identity(PERSON, PLAIN)["data"]["content"],
                "This channel mirrors no room of the Dereth Network."
            );
            assert!(n.identity(PERSON, ADMINS)["data"]["content"]
                .as_str()
                .is_some_and(|c| c.contains("for everyone to read")));
            let calls = n.d.lock().expect("the stand-in").calls.clone();
            assert!(
                calls
                    .iter()
                    .all(|c| !(c.method == "POST" && c.path.ends_with("/messages"))),
                "nothing said where anyone else sees it"
            );
            assert!(n.lines_in(AAS).is_empty() && n.lines_in(ADMINS).is_empty());
        }

        /// Spec: hub: `/dn say` asks the chosen character's world to say the line, marked as said
        /// from Discord, and answers the person at once, by deferring, alone; the world's line goes
        /// on with its mark to the other worlds and to every mirror of the room, the channel it was
        /// said in among them, under the character's address and the mark; the person's answer is
        /// edited to say it was said.
        #[test]
        fn a_line_said_from_discord_goes_through_its_characters_world_to_every_mirror_marked() {
            let mut n = Net::new();
            n.choose(PERSON, AAS, "Ann@aa");
            let (answer, token) = n.say(PERSON, AAS, "evening **all** @everyone");
            assert_eq!(answer, json!({ "type": 5, "data": { "flags": 64 } }));
            let SayMessage::Ask {
                room,
                who,
                text,
                via,
                ..
            } = n.world_answers(SayOutcome::Said, None)
            else {
                unreachable!("a say")
            };
            assert_eq!(
                (room.as_str(), who.to_string(), text.as_str(), via.as_str()),
                (
                    "global",
                    "Ann@aa".into(),
                    "evening **all** @everyone",
                    "discord"
                ),
                "to Ann's world alone, marked"
            );
            let marked = n
                .at_bb
                .expect(WAIT, "the line", |m| match m {
                    Message::RoomLine {
                        from, text, via, ..
                    } => Some((from.to_string(), text.clone(), via.clone())),
                    _ => None,
                })
                .expect("the line");
            assert_eq!(
                marked,
                (
                    "Ann@aa".into(),
                    "evening **all** @everyone".into(),
                    Some("discord".into())
                )
            );
            assert_eq!(n.edited(&token), "Said as Ann@aa.");
            for channel in [AAS, ADMINS] {
                let lines = within("the line in the mirror", || {
                    let l = n.lines_in(channel);
                    (!l.is_empty()).then_some(l)
                });
                assert_eq!(
                    lines,
                    [(
                        "Ann@aa (via /dn)".to_owned(),
                        "evening \\*\\*all\\*\\* @\u{200B}everyone".to_owned()
                    )],
                    "{channel}"
                );
            }
        }

        /// Spec: hub: `/dn say` from a person with one character on the world speaks as it, with no
        /// choice made; one with several and none chosen is answered with the menu of them, and
        /// its choice there says the line and is kept; the menu's line is said once.
        #[test]
        fn a_line_is_said_as_the_one_character_or_the_one_chosen_from_the_menu() {
            let mut n = Net::new();
            // The other person links Cy@aa alone.
            let (answer, token) = n.say(OTHER, AAS, "evening");
            assert_eq!(answer["type"], 5, "{answer}");
            let SayMessage::Ask { who, .. } = n.world_answers(SayOutcome::Said, None) else {
                unreachable!("a say")
            };
            assert_eq!(who.to_string(), "Cy@aa");
            assert_eq!(n.edited(&token), "Said as Cy@aa.");
            // The person links Ann@aa and Bo@aa, and has chosen neither.
            let (menu, _) = n.say(PERSON, AAS, "hello");
            assert_eq!(menu["type"], 4, "{menu}");
            assert_eq!(
                menu["data"]["content"],
                "Choose which of your characters on World aa you speak as. Your line is said as the one you choose."
            );
            let select = &menu["data"]["components"][0]["components"][0];
            let custom_id = select["custom_id"].as_str().expect("a menu").to_owned();
            assert!(custom_id.starts_with("dn-say:"), "{custom_id}");
            let options: Vec<&str> = select["options"]
                .as_array()
                .expect("options")
                .iter()
                .filter_map(|o| o["value"].as_str())
                .collect();
            assert_eq!(options, ["Ann@aa", "Bo@aa"]);
            assert!(
                n.at_aa
                    .expect_say(Duration::from_millis(300), "a say", |m| Some(m.clone()))
                    .is_err(),
                "no world is asked before the choice"
            );
            let (said, token) = n.choose_in(PERSON, AAS, &custom_id, "Bo@aa");
            assert_eq!(
                said,
                "You speak as Bo@aa on World aa now. Saying your line..."
            );
            let SayMessage::Ask { who, text, .. } = n.world_answers(SayOutcome::Said, None) else {
                unreachable!("a say")
            };
            assert_eq!((who.to_string(), text.as_str()), ("Bo@aa".into(), "hello"));
            assert_eq!(
                n.edited(&token),
                "You speak as Bo@aa on World aa now. Said as Bo@aa."
            );
            // The choice is kept.
            let (answer, _) = n.say(PERSON, AAS, "again");
            assert_eq!(answer["type"], 5, "{answer}");
            let SayMessage::Ask { who, .. } = n.world_answers(SayOutcome::Said, None) else {
                unreachable!("a say")
            };
            assert_eq!(who.to_string(), "Bo@aa");
            // The menu's line was said once.
            let (again, _) = n.choose_in(PERSON, AAS, &custom_id, "Ann@aa");
            assert_eq!(
                again,
                "You speak as Ann@aa on World aa now. Your line waited too long and was not said. Say it again with /dn say."
            );
            assert!(
                n.at_aa
                    .expect_say(Duration::from_millis(300), "a say", |m| Some(m.clone()))
                    .is_err(),
                "said once"
            );
        }

        /// Spec: hub: from an admins' mirror, in a channel that mirrors nothing, or by a Discord
        /// user who links no character there, nothing is said and no world is asked; the person is
        /// told why, alone.
        #[test]
        fn nothing_is_said_outside_a_worlds_mirror_or_by_one_who_links_no_character() {
            let mut n = Net::new();
            n.choose(PERSON, AAS, "Ann@aa");
            let (answer, _) = n.say(PERSON, ADMINS, "hello");
            assert!(answer["data"]["content"]
                .as_str()
                .is_some_and(|c| c.starts_with(
                    "This channel mirrors the network's Global room for everyone to read"
                )));
            let (answer, _) = n.say(PERSON, PLAIN, "hello");
            assert_eq!(
                answer["data"]["content"],
                "This channel mirrors no room of the Dereth Network."
            );
            let (answer, _) = n.say(STRANGER, AAS, "hello");
            assert!(answer["data"]["content"]
                .as_str()
                .is_some_and(|c| c.starts_with("You have no character linked on World aa.")));
            let (answer, _) = n.say(PERSON, AAS, "  \n ");
            assert_eq!(answer["data"]["content"], "There is nothing to say.");
            assert!(
                n.at_aa
                    .expect_say(Duration::from_millis(500), "a say", |m| Some(m.clone()))
                    .is_err(),
                "no world is asked"
            );
            assert!(n.lines_in(AAS).is_empty() && n.lines_in(ADMINS).is_empty());
        }

        /// Spec: hub: the world's refusal reaches the person with its reason, and nothing is said;
        /// a world that says it has no such character ends the choice; a world whose link is down
        /// is not asked.
        #[test]
        fn the_worlds_refusal_reaches_the_person() {
            let mut n = Net::new();
            n.choose(PERSON, AAS, "Ann@aa");
            let (_, token) = n.say(PERSON, AAS, "hello");
            n.world_answers(SayOutcome::Refused, Some("gagged"));
            assert_eq!(
                n.edited(&token),
                "Ann@aa did not say it: it is gagged on World aa."
            );
            let (_, token) = n.say(PERSON, AAS, "hello");
            n.world_answers(SayOutcome::Refused, Some("no_character"));
            assert_eq!(
                n.edited(&token),
                "Ann@aa did not say it: World aa has no such character now. Choose another with /dn identity."
            );
            let (answer, _) = n.say(PERSON, AAS, "hello");
            assert!(
                answer["data"]["components"][0]["components"][0]["custom_id"]
                    .as_str()
                    .is_some_and(|c| c.starts_with("dn-say:")),
                "the choice ended: the menu again: {answer}"
            );
            assert!(n.lines_in(AAS).is_empty() && n.lines_in(ADMINS).is_empty());
            n.choose(PERSON, AAS, "Bo@aa");
            // A second link replaces the first, and goes: the world has no link.
            link(&n.f, "aa", &n.key_aa, &["global"]).close();
            within("the world's link gone", || {
                (!n.f.running.linked().contains("aa")).then_some(())
            });
            let (_, token) = n.say(PERSON, AAS, "hello");
            assert_eq!(n.edited(&token), "World aa can't be reached right now.");
        }

        /// Spec: hub: every interaction comes from Discord's own addresses, so one Discord signed
        /// takes nothing from its client address's allowance, and more than the allowance in a
        /// minute are all answered; a request there that Discord did not sign takes from it, and
        /// is answered `429` once it is spent, as the address's other requests are then.
        #[test]
        fn signed_interactions_take_nothing_from_the_addresss_allowance_and_others_do() {
            // An allowance that gives back a request only every second, so that a slow machine is
            // not given one back while the test spends it.
            let mut n = Net::with_allowance(60);
            for _ in 0..200 {
                let id = n.id();
                let r =
                    n.post(&json!({ "id": id, "application_id": APP, "type": 1, "token": "t" }));
                assert_eq!(r.status, 200, "{r:?}");
            }
            let menu = n.identity(PERSON, AAS);
            assert_eq!(menu["type"], 4, "a command too: {menu}");
            let unsigned = || {
                call(
                    n.f.addr(),
                    "POST",
                    "/v1/discord/interactions",
                    &[("Content-Type", "application/json")],
                    r#"{"type":1}"#,
                )
                .status
            };
            // Up to the first `429` only, so the allowance has no time to refill before the next
            // request.
            let mut statuses: Vec<u16> = Vec::new();
            while statuses.len() < 250 && statuses.last() != Some(&429) {
                statuses.push(unsigned());
            }
            let first = statuses
                .iter()
                .position(|s| *s == 429)
                .expect("unsigned requests spend the allowance");
            assert!(
                first > 0 && statuses[..first].iter().all(|s| *s == 401),
                "{statuses:?}"
            );
            // One may yet be given back before the next refusal; five in a row can't all be.
            assert!(
                (0..5).any(|_| call(n.f.addr(), "GET", "/v1/config", &[], "").status == 429),
                "the address's allowance is spent"
            );
            let id = n.id();
            let r = n.post(&json!({ "id": id, "application_id": APP, "type": 1, "token": "t" }));
            assert_eq!(r.status, 200, "Discord's are still answered: {r:?}");
        }

        /// Spec: hub: a person's lines from Discord are held to its own pace, a burst of three and
        /// then one every five seconds, before any world is asked.
        #[test]
        fn a_persons_lines_are_held_to_its_pace() {
            let mut n = Net::new();
            n.choose(PERSON, AAS, "Ann@aa");
            for _ in 0..3 {
                let (answer, _) = n.say(PERSON, AAS, "hello");
                assert_eq!(answer["type"], 5);
                n.world_answers(SayOutcome::Said, None);
            }
            let (answer, _) = n.say(PERSON, AAS, "hello");
            assert_eq!(
                answer["data"]["content"],
                "You are speaking too quickly. Wait a moment and try again."
            );
        }
    }
}
